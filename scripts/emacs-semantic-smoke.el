;;; emacs-semantic-smoke.el --- drive typelisp-mode's eglot semantic tokens -*- lexical-binding: t; -*-

;; The Emacs counterpart of `scripts/lsp-semantic-smoke.py': that one checks
;; the server's answer on the wire, this one checks that the answer is actually
;; *drawn*.  Which client draws it depends on the Emacs: an `eglot' with
;; `eglot-semantic-tokens-mode' (Emacs 31 and later) has its own, and
;; `typelisp-mode' must then stand aside; an older one has none, and the mode's
;; own client is what draws.  Only a real eglot connection exercises either.
;;
;; The document it opens (`todo-cli''s `store.typl') defines no types of its
;; own: `todo-item' reaches it through `(use model::todo-item)'.  So every
;; overlay reported below is a type the buffer-local fallback could not have
;; found.
;;
;;   emacs --batch -Q -L editor/emacs -l scripts/emacs-semantic-smoke.el
;;
;; Expects `target/debug/typl-lsp' to be built.  Exits non-zero on failure.

(require 'cl-lib)
(require 'eglot)
(require 'typelisp-mode)

(defvar typelisp-smoke-root
  (file-name-directory (directory-file-name
                        (file-name-directory (or load-file-name buffer-file-name))))
  "The typelisp checkout this script lives in.")

(defun typelisp-smoke--wait (predicate seconds what)
  "Pump events until PREDICATE returns non-nil, or fail after SECONDS.
WHAT names the thing being waited for, for the failure message."
  (let ((deadline (+ (float-time) seconds)))
    (while (and (not (funcall predicate)) (< (float-time) deadline))
      (accept-process-output nil 0.1))
    (unless (funcall predicate)
      (message "FAIL: timed out waiting for %s" what)
      (kill-emacs 1))))

;; An eglot with a client of its own: the mode's client stays off, and eglot's
;; font-lock pass paints `todo-item' with a semantic face.
(defun typelisp-smoke--eglot-draws ()
  (when typelisp-semantic-tokens-mode
    (message "FAIL: typelisp-semantic-tokens-mode is on beside eglot's own client")
    (kill-emacs 1))
  (unless eglot-semantic-tokens-mode
    (message "FAIL: eglot-semantic-tokens-mode is not on")
    (kill-emacs 1))
  (let ((uses nil))
    (save-excursion
      (goto-char (point-min))
      (while (search-forward "todo-item" nil t)
        (push (match-beginning 0) uses)))
    (unless uses
      (message "FAIL: the document no longer mentions todo-item")
      (kill-emacs 1))
    (typelisp-smoke--wait
     (lambda ()
       (font-lock-ensure)
       (cl-some (lambda (pos)
                  (let ((face (get-text-property pos 'face)))
                    (memq 'eglot-semantic-struct (if (listp face) face (list face)))))
                uses))
     20 "eglot to paint a semantic token")
    (message "eglot painted todo-item at %d of %d uses"
             (cl-count-if (lambda (pos)
                            (let ((face (get-text-property pos 'face)))
                              (memq 'eglot-semantic-struct (if (listp face) face (list face)))))
                          uses)
             (length uses))))

;; An eglot without one: the mode's own client turns itself on and draws
;; overlays, each covering exactly the imported type's name.
(defun typelisp-smoke--mode-draws ()
  (unless typelisp-semantic-tokens-mode
    (message "FAIL: typelisp-semantic-tokens-mode did not turn itself on")
    (kill-emacs 1))
  (let ((legend (typelisp--semantic-legend (eglot-current-server))))
    (unless legend
      (message "FAIL: the server advertised no semanticTokens legend")
      (kill-emacs 1))
    (message "legend: %S" legend))
  (typelisp--semantic-refresh)
  (typelisp-smoke--wait (lambda () typelisp--semantic-overlays) 20 "semantic tokens")
  (message "overlays: %d" (length typelisp--semantic-overlays))
  (dolist (ov (reverse typelisp--semantic-overlays))
    (let ((start (overlay-start ov)))
      (message "  L%d c%d %S -> %S"
               (line-number-at-pos start)
               (save-excursion (goto-char start) (current-column))
               (buffer-substring-no-properties start (overlay-end ov))
               (overlay-get ov 'face))))
  (dolist (ov typelisp--semantic-overlays)
    (let ((text (buffer-substring-no-properties (overlay-start ov) (overlay-end ov))))
      (unless (equal text "todo-item")
        (message "FAIL: overlay covers %S, expected \"todo-item\"" text)
        (kill-emacs 1)))))

(let* ((server (expand-file-name "target/debug/typl-lsp" typelisp-smoke-root))
       (file (expand-file-name "examples/projects/todo-cli/src/store.typl"
                               typelisp-smoke-root)))
  (unless (file-executable-p server)
    (message "FAIL: %s is not built (cargo build --bin typl-lsp)" server)
    (kill-emacs 1))
  ;; A short idle delay: this script drives the timer by hand rather than
  ;; waiting on the interactive default.
  (setq typelisp-semantic-tokens-idle-delay 0.1)
  (find-file file)
  (unless (derived-mode-p 'typelisp-mode)
    (message "FAIL: %s did not open in typelisp-mode" file)
    (kill-emacs 1))
  ;; The project has to be the very one the buffer itself reports: eglot
  ;; registers the connection under the project object it is handed, and
  ;; `eglot-current-server' looks it up again from the buffer -- hand it a
  ;; separately-built transient project and the buffer never finds its own
  ;; server.
  (eglot 'typelisp-mode
         (or (project-current) (cons 'transient (file-name-directory file)))
         'eglot-lsp-server
         (list server)
         "typelisp")
  (typelisp-smoke--wait (lambda () (eglot-current-server)) 20 "the eglot connection")
  ;; `eglot-managed-mode-hook' has run by now, so whichever client applies is on.
  (if (fboundp 'eglot-semantic-tokens-mode)
      (typelisp-smoke--eglot-draws)
    (typelisp-smoke--mode-draws))
  ;; The buffer-local fallback must have retired now that the server is
  ;; answering, whichever client carries the answer.
  (unless (typelisp--server-highlights-types-p)
    (message "FAIL: the buffer-local fallback did not stand down")
    (kill-emacs 1))
  (when (typelisp--match-local-type (point-max))
    (message "FAIL: the fallback matcher still fires while the server answers")
    (kill-emacs 1))
  (message "OK"))

;;; emacs-semantic-smoke.el ends here

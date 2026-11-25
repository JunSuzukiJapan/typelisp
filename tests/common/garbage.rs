//! Programs that make garbage without taking a cons cell, for checking that
//! it is collected: each makes far more than the memory it is given.

/// Each program's name, its definitions, and the call of it that answers 0
/// when everything kept is still there. `n` in the call is how many rounds.
pub const PROGRAMS: [(&str, &str, &str); 7] = [
    (
        "strings",
        r#"
(defun garbage ((n int)) int
  (let ((kept (append "ke" "pt")) (total 0))
    (dotimes (i n) (setf total (+ total (length (append "a string to throw away, " (append kept "!"))))))
    (if (and (= total (* n 29)) (equal kept "kept")) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "vectors",
        r#"
(defun garbage ((n int)) int
  (let ((kept (Vector::filled 10 7)) (total 0))
    (dotimes (i n)
      (let ((v (Vector::filled 8 i)))
        (push v i)
        (setf total (+ total (len v)))))
    (if (and (= total (* n 9)) (= (get kept 9) 7)) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "grown-vectors",
        r#"
(defun garbage ((n int)) int
  (let ((total 0))
    (dotimes (i (ash n -7))
      (let ((v (the Vector<int> (Vector::new))))
        (dotimes (k 1000) (push v k))
        (setf total (+ total (len v)))))
    (if (= total (* 1000 (ash n -7))) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "structs",
        r#"
(defstruct point (x int) (y string))
(defun garbage ((n int)) int
  (let ((kept (make-instance 'point :x 5 :y (append "a" "b"))) (total 0))
    (dotimes (i n)
      (let ((p (make-instance 'point :x i :y kept::y)))
        (setf total (+ total (length p::y)))))
    (if (and (= total (* 2 n)) (equal kept::y "ab")) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "closures",
        r#"
(defun garbage ((n int)) int
  (let ((total 0) (text (append "a" "b")))
    (dotimes (i n)
      (let ((f (lambda ((k int)) int (+ k (+ i (length text))))))
        (setf total (+ total (- (f 1) i)))))
    (if (= total (* 3 n)) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "recursion",
        r#"
(defun down ((n int) (text string)) int
  (if (= n 0) (length text) (down (- n 1) (append "x" (substring text 0 20)))))
(defun tree ((depth int) (text string)) int
  (if (= depth 0)
      (length (append text text))
      (+ (tree (- depth 1) (append text "")) (tree (- depth 1) (append "" text)))))
(defun rounds ((n int)) int (if (<= n 1) 0 (+ 1 (rounds (ash n -1)))))
(defun garbage ((n int)) int
  (let ((depth (rounds n)))
    (if (and (= (tree depth "0123456789") (* 20 (ash 1 depth)))
             (= (down (if (< n 1000) n 1000) "a text of some length, cut") 21)) 0 1)))
"#,
        "(garbage N)",
    ),
    (
        "bignums",
        r#"
(defun garbage ((n int)) int
  (let ((kept (expt 3 200)) (total 0))
    (dotimes (i n)
      (when (= (+ kept i) (- (+ kept (+ i 1)) 1)) (setf total (+ total 1))))
    (if (and (= total n) (= kept (expt 3 200))) 0 1)))
"#,
        "(garbage N)",
    ),
];

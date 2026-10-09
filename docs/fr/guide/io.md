<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# E/S de fichiers, flux et réseau

Ce guide présente les bases de la lecture et de l'écriture de fichiers, des noms de chemin et de la communication
par sockets. Les listes de fonctions se trouvent dans [Flux et fichiers](../reference/functions/streams-files.md)
et [Réseau](../reference/functions/network.md).

## 1. Les échecs reviennent sous forme de `Result`

Les opérations qui peuvent échouer selon l'environnement, comme ouvrir un fichier ou se connecter, renvoient un
`Result`. Un fichier manquant n'est pas une erreur du programme ; il n'y a donc pas de `panic`. Séparez les
issues avec `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; si le fichier manque :
;; error: config.txt: No such file or directory (os error 2)
```

Quand on sait qu'une opération ne peut pas échouer, ou dans un petit script où s'arrêter en cas d'échec convient,
`unwrap` extrait la valeur. Si c'est un `Err`, c'est un panic.

## 2. Lire et écrire un fichier entier

Les fonctions les plus simples traitent le fichier entier d'un coup.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. Lire et écrire avec des flux

Pour lire ou écrire petit à petit, ouvrez un flux avec `with-open-file`. Quelle que soit la façon dont le corps est
quitté, le flux est fermé. La valeur est `Result<valeur du corps, FileError>`.

```lisp
;; écrire
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; lire une ligne à la fois
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Il y a trois directions d'ouverture : `direction-input` (lecture), `direction-output` (écriture ; le contenu
  existant est effacé) et `direction-append` (ajout à la fin).
- `read-line` renvoie `none` à la fin du fichier.
- Si vous ouvrez un fichier avec `open-file` au lieu de `with-open-file`, appelez toujours `close`. Le GC ne ferme
  pas les flux.

Pour lire et écrire des octets, ouvrez le fichier avec `open-binary-input` / `open-binary-output` et utilisez
`read-byte` / `write-byte`. Les flux de caractères et les flux d'octets sont des types différents ; tenter de lire
des octets dans un flux de caractères est donc une erreur de type.

### Utiliser une chaîne comme flux

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Entrée et sortie standard

`*standard-input*`, `*standard-output*` et `*error-output*` sont aussi des flux. `(read-line *standard-input*)` lit
une ligne.

### Rendre génériques les fonctions de lecture et d'écriture

Chaque sorte de flux est un type à part, mais les opérations communes sont regroupées dans des traits. Une fonction
qui prend son argument avec `(where (CharInput S))` peut lire depuis un fichier, une chaîne ou une socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Noms de chemin

Les fonctions qui prennent un nom de fichier acceptent une chaîne ou un `pathname`. Utilisez un `pathname` pour
découper un chemin en parties ou pour en construire un.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; remplacer seulement l'extension
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; placer un nom de fichier dans un répertoire
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Opérations sur le système de fichiers :

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; le créer avec ses parents
(probe-file "out/deep")                           ; => true (il existe)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => la liste du contenu
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Le séparateur est toujours `/`. Il n'y a pas de composants hôte, périphérique ou version comme dans les noms de
chemin de Common Lisp, ni de jokers ni de noms de chemin logiques.

## 5. TCP

Une connexion par socket est aussi un flux ; `read-line` et `write-line` y fonctionnent donc tels quels.

### Serveur

Le schéma de base consiste à lancer une tâche par connexion. `accept` et `read-line` n'arrêtent **que cette
tâche** jusqu'à l'arrivée de données ; le traitement des autres connexions continue donc.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; l'autre côté a fermé
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` n'accepte que les connexions venant de cette machine, et `"0.0.0.0"` les accepte sur toutes les
interfaces. Pour les tâches, voir le
[chapitre 12 de la référence de la syntaxe](../reference/syntax.md#12-concurrence-tâches).

### Client

`with-connection` se connecte, exécute son corps et ferme la connexion à la fin. La valeur est
`Result<valeur du corps, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Pour limiter la durée d'une opération, passez `:timeout` (en secondes).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; mettre une horloge sur la prochaine lecture
```

### Quand l'autre côté se déconnecte

Si l'autre côté réinitialise la connexion ou la coupe en cours de route, il n'y a pas de `panic`. Les lectures
suivantes renvoient `none`, et les écritures sont silencieusement ignorées. Pour savoir si la connexion a été
fermée proprement ou a échoué, consultez `(socket-error c)`.

## 6. Résolution de noms (DNS)

Il n'existe pas de fonction dédiée à la résolution de noms. Passer un nom d'hôte à `tcp-connect`, `tls-connect` ou
`send-to` le résout à l'intérieur de ces fonctions. La résolution a lieu sur un autre thread ; les autres tâches
continuent donc de s'exécuter pendant l'attente. Si un nom a plusieurs adresses, elles sont essayées tour à tour.

Si le nom ne peut pas être résolu, un `Err` est renvoyé.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` s'utilise comme `tcp-connect` et effectue la négociation TLS après la connexion. Le certificat du
serveur est vérifié avec les certificats racines standard. Le résultat est un `socket-stream` ordinaire ; lecture
et écriture fonctionnent donc comme avec TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Un serveur TLS se crée en passant à `tls-listen` les fichiers du certificat et de la clé privée (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; le serve de TCP fonctionne tel quel
          ((err e) (println "accept: ~a" (message e))))))
```

Pour essayer avec un certificat auto-signé, passez `:ca-file "cert.pem"` côté client afin qu'il fasse confiance à
ce certificat. Le TLS mutuel et le service de plusieurs sites depuis un seul listener sont traités dans
[Réseau](../reference/functions/network.md#2-tcp--tls--domaine-unix).

## 8. UDP

UDP n'est pas un flux ; il envoie et reçoit un datagramme à la fois. Les données sont une suite d'octets
(`Vector<int>`) ; on convertit depuis et vers des chaînes avec `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; même pour envoyer, il faut une socket ; 0 laisse le port au système
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` est l'`ip:port` de l'expéditeur, utilisable tel quel comme destination de `send-to`.

## 9. Sockets du domaine Unix

Passez le chemin du fichier de socket à `unix-listen` / `unix-connect`. La valeur obtenue est le même
`socket-stream` qu'avec TCP. `unix-listen` renvoie un `Err` si le fichier existe déjà. Fermer le listener supprime
le fichier.

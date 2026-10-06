<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Réseau (TCP / TLS / domaine Unix / UDP)

Les sockets font partie des [flux](streams-files.md). Une connexion est vue sous **deux vues de la même
connexion**, `socket-stream` (caractères) et `socket-byte-stream` (octets), et
`read-line`/`write-line`/`read-byte`/`format` y fonctionnent tels quels. Un listener est un `socket-listener`.
**TCP, TLS et les sockets du domaine Unix partagent un même type** (comme le `net.Conn` de Go) : une fois connectés,
lecture et écriture sont identiques, et seule la façon de les créer diffère. UDP n'est pas un flux mais des
datagrammes (`udp-socket`).

**Ce qui attend, c'est la tâche, pas le thread.** `accept`, `read-line`, `write-string`, `tcp-connect` (y compris
la résolution de noms) et `recv-from` arrêtent tous *cette tâche* s'ils ne sont pas prêts (comme `sleep`/`recv`),
et les autres tâches continuent. C'est pourquoi un serveur peut s'écrire sous la même forme qu'en Go,
`(task (serve c))` par connexion ([Référence de la syntaxe 12.5](../syntax.md#125-où-les-tâches-changent)).

## 1. Types

| Type | Traits implémentés | Comment en obtenir un |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes` : `Vector<int>`, `from` : `string`) | `recv-from` |
| `NetError` | `Error` | Le `Err` des fonctions ci-dessous |

Un type de flux a un seul type d'élément (pour la même raison que `file-stream`/`binary-file-stream`) ; caractères
et octets sont donc des types différents. `byte-stream-of`/`char-stream-of` renvoient des valeurs qui désignent
**la même connexion** et partagent le tampon de réception : c'est ainsi qu'on écrit par exemple HTTP, où les
en-têtes sont lus comme caractères et le corps comme octets. Lire un octet juste après l'`unread-char` d'un
caractère est une erreur (la même règle que pour les fichiers).

## 2. TCP / TLS / domaine Unix

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Se connecte. `host` peut être un nom ou une adresse. Si un nom a plusieurs adresses, elles sont essayées tour à tour (`localhost` est `::1` et `127.0.0.1`). Un échec de résolution, une connexion refusée ou le dépassement de `:timeout` secondes donnent `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS après `tcp-connect`. Le certificat est vérifié par rapport au nom `host` (`:server-name` si le nom à vérifier diffère de la destination) avec les certificats racines de Mozilla. Avec `:ca-file` (PEM), il ne fait confiance **qu'aux certificats qu'il contient** (une CA privée, ou le certificat même que présente votre propre `tls-listen`). `:cert-file`/`:key-file` (les deux ou aucun) sont notre certificat, présenté quand le serveur le demande (TLS mutuel). La négociation s'achève ici ; si le certificat du serveur ne passe pas, cet appel renvoie `Err`. Le résultat est un `socket-stream` ordinaire |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Se connecte à la socket du domaine Unix `path`. Elle est locale ; il n'y a donc ni attente de négociation ni délai |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Écoute. `"127.0.0.1"` ne concerne que cette machine, `"0.0.0.0"` toutes les interfaces. Passer `0` comme `port` laisse le système choisir |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | La version TLS de `tcp-listen`. `cert-file` est la chaîne de certificats (PEM, notre certificat en premier) et `key-file` la clé privée. Les deux sont lus et contrôlés ici ; une clé qui ne correspond pas donne donc `Err` dès cet appel, pas au premier client. `accept` revient **avant la négociation**, et la première lecture ou écriture de la tâche qui traite la connexion achève la négociation (comme le `tls.Conn` de Go) ; un client lent à négocier ne retarde donc pas les autres `accept`. Avec `:client-ca` (PEM), il **exige de chaque client** un certificat émis par une CA qu'il contient (TLS mutuel). Sans, aucun n'est demandé |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Écoute sur `path`. **`Err` si le fichier existe déjà** (il pourrait appartenir à un autre processus en cours ; il n'est donc pas remplacé silencieusement). `close` supprime le fichier |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | La connexion suivante. Arrête la tâche jusqu'à ce qu'il en arrive une. Abandonne avec `Err` après `:timeout` secondes |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Jusqu'à ce qu'on puisse lire sans attendre, ou `secs` secondes. `true` signifie le premier cas (y compris des données déjà en tampon). La façon de mettre une horloge sur une lecture : `(if (wait-readable c 5.0) (read-line c) ...)`. Ce qui est promis, c'est que **la prochaine lecture ne s'arrête pas** ; `read-line` peut attendre le reste de la ligne |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Jusqu'à ce qu'on puisse écrire, ou `secs` secondes |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Ajoute un certificat de plus à un listener `tls-listen`, présenté aux clients qui demandent `name` (SNI) : plusieurs sites sur un listener. On contrôle ici que la chaîne vaut pour `name`, sinon cet appel renvoie `Err`. Les clients qui demandent un nom que personne n'a ajouté, ou aucun nom, reçoivent le certificat de `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Le sujet du certificat du pair (`CN=client,O=Example,C=JP`, forme RFC 4514, le plus spécifique d'abord). C'est ainsi qu'un serveur en TLS mutuel apprend « qui s'est connecté » (après la première lecture, qui achève la négociation). Côté client, le nom du certificat du serveur. `none` pour les connexions simples, avant la négociation ou si le pair n'a présenté aucun certificat |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Sur une connexion TLS côté serveur, le nom demandé par le client (SNI). C'est ainsi qu'un serveur avec plusieurs sites ajoutés par `tls-add-certificate` apprend « quel site ». `none` pour les connexions simples, côté client ou sans nom (connexion par adresse) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Désactive Nagle (`TCP_NODELAY`). `write-string` va jusqu'à la socket à chaque fois ; avec Nagle actif, une réponse écrite en deux parties, en-tête et corps, attend donc l'ACK retardé du pair : utilisez `true` pour les protocoles requête/réponse. Les sockets du domaine Unix n'ont pas de Nagle, et l'appel réussit simplement |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sonde le pair pendant l'inactivité (`SO_KEEPALIVE`). Détecte un pair qui a disparu sans fermer (un câble débranché, un hôte arrêté) et réinitialise la connexion. La période par défaut du système est longue (souvent 2 heures) ; combinez-le donc avec `set-keepalive-period` ci-dessous. Les sockets du domaine Unix ne l'ont pas ; c'est donc un panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Les secondes d'inactivité avant la première sonde et l'intervalle entre sondes (secondes entières, au moins 1). Comme le `SetKeepAlivePeriod` de Go, les deux reçoivent la même valeur |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Si **le pair** a cassé cette connexion, son premier échec (une réinitialisation, une alerte TLS, une coupure pendant une écriture). `none` si tout va bien : un EOF dû à une fermeture propre du pair n'est pas un échec. Voir « Échecs du pair » ci-dessous |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | Le `host:port` de notre côté. La façon d'apprendre le port choisi après `(tcp-listen h 0)`. Pour les sockets Unix, le chemin (l'extrémité qui se connecte est `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | Le `host:port` du pair, ou le chemin pour les sockets Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Ferme seulement le côté émission (une demi-fermeture). Le pair lit EOF, et ce côté peut encore lire. Le signal « j'ai envoyé toute la requête ». Pour TLS, envoie aussi `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | La version octets de la même connexion |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | La version caractères de la même connexion |
| `close` | `(close s)` | `Stream` | Envoie le tampon, puis ferme. Le GC ne ferme pas |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Se connecter, exécuter le corps, fermer. `Result<valeur du corps, NetError>` (la même forme que `with-open-file`) |

`write-string`/`write-line` **reviennent une fois tout écrit** (comme le `net.Conn.Write` de Go). Pour regrouper de
petites écritures, accumulez-les dans un `string-output-stream` et écrivez une fois. `listen` n'est `true` que
lorsqu'il y a quelque chose dans le tampon de réception ; `read-char-no-hang` fonctionne donc comme son nom
l'indique.

**Les échecs du pair ne déclenchent pas de panic.** Si l'autre extrémité réinitialise la connexion, rejette la
négociation TLS ou la coupe au milieu d'une écriture, ce n'est pas une erreur de ce programme ; le serveur ne
s'arrête donc pas avec ses autres clients. Le premier échec est enregistré sur la connexion ; les lectures
suivantes renvoient `none` (comme un EOF), et les écritures ne vont nulle part et reviennent silencieusement. Pour
les distinguer, utilisez `socket-error`, sur le modèle du `bufio.Scanner.Err` de Go (`read-item` est une `Option`
et n'a pas d'autre canal pour signaler). Seules les erreurs du programme lui-même déclenchent un panic (une poignée
fermée, lire un octet juste après `unread-char`).

**Les délais sont des arguments explicites ou `wait-readable`.** Il n'existe pas de forme, comme le
`SetReadDeadline` de Go, où un flux porte une échéance et où `read-line` échoue : `read-item` renvoie
`Option<Item>` et n'a aucun moyen de renvoyer une erreur. Une horloge est nécessaire à trois endroits, la
connexion, l'acceptation et « la prochaine lecture », et chacun a un argument pour cela.

```lisp
;; serveur : une tâche par connexion
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; client
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; serveur TLS (le certificat peut se créer par exemple avec
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; le serve ci-dessus tel quel ; le premier read-line est la négociation
          ((err e) (println "accept: ~a" (message e))))))
;; son client : se connecter en faisant confiance à son propre certificat
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; TLS mutuel : le serveur exige un certificat client émis par ca.pem, et le client en présente un
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; côté serveur, après le premier read-line : qui s'est connecté
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; deux sites sur un listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; sur la connexion, (requested-server-name c) dit lequel
```

En TLS mutuel, un client qui ne présente pas de certificat (ou un qui ne passe pas) achève sa propre partie de la
négociation avant que le serveur ne décide, sous TLS 1.3 ; `tls-connect` renvoie donc `Ok`, et **la première lecture
renvoie `none`** (avec l'alerte dans `socket-error`). Côté serveur, la première lecture de la même connexion
renvoie aussi `none`. Aucun des deux côtés ne déclenche de panic.

## 3. UDP

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Crée une socket. Nécessaire même pour simplement envoyer (`port` vaut `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Envoie un datagramme. Les noms sont résolus. On ne sait pas s'il est arrivé (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Le datagramme suivant. `from` est `ip:port` et peut être passé tel quel comme `host` de `send-to` |

On convertit entre chaînes et suites d'octets avec `string->utf8` / `utf8->string`
([Chaînes](collections.md#1-chaînes-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Ce qui n'existe pas

- Un moyen de lire autre chose que **le sujet** dans le certificat du pair (SAN, période de validité, émetteur).
- **Attendre à la fois une socket et un canal avec `select`.** Comme en Go, écrivez-le sous la forme « lancer une
  tâche qui lit et lui faire alimenter un canal ».
- **HTTP/2**. Les **datagrammes** du domaine Unix (`SOCK_DGRAM`).
- **Des échéances portées par un flux** (pour les raisons ci-dessus, les horloges sont des arguments).

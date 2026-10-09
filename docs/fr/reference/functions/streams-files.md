<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Flux et fichiers

Les traits et méthodes de flux, les types de flux concrets, les opérations sur les fichiers et les noms de chemin.
Les sockets réseau sont aussi des flux et sont traitées dans [Réseau](network.md).

## 1. La hiérarchie des traits

Ce que CL exprime par une hiérarchie de classes s'exprime ici par une **hiérarchie de traits**. La direction
(entrée / sortie) comme le type des éléments sont décidés **statiquement** ; il n'y a donc pas besoin de demander à
l'exécution « peut-on lire ce flux ? ».

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; entrée de caractères
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; sortie de caractères
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; entrée qui peut remettre un caractère
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; entrée d'octets
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; sortie d'octets
```

Une fonction qui lit des caractères accepte n'importe quel type de flux, intégré ou défini par l'utilisateur, si elle
prend `(where (CharInput S))` ou `:dyn CharInput`.

## 2. Méthodes

Chaque méthode de `CharInput` a une implémentation par défaut. Une implémentation n'écrit que `read-item`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | L'élément suivant. `none` à la fin. **La seule méthode à implémenter** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Le caractère suivant |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Jusqu'au prochain saut de ligne (le saut de ligne est consommé et retiré). Une dernière ligne qui ne se termine pas par un saut de ligne est aussi renvoyée |
| `read-all` | `(read-all s)` | `(S)→string` | Tout ce qui reste |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Seulement un caractère déjà disponible. `none` plutôt que d'attendre |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Empile jusqu'à `n` caractères sur `v` et renvoie combien ont réellement été lus. Moins que `n` seulement à la fin |

`listen` se trouve dans `InputStream` (le parent de `CharInput`) :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Si la prochaine lecture peut répondre sans attendre. La valeur par défaut est `false`, **le côté qui ne ment jamais** : `true` serait une supposition, et une mauvaise supposition ferait bloquer `read-char-no-hang`. Tous les flux intégrés la redéfinissent. **Pour les flux définis par l'utilisateur qui ne la redéfinissent pas, `read-char-no-hang` renvoie toujours `none`** |

`PeekInput` (qui hérite de `CharInput`) ajoute **la remise d'un caractère**. Seul le flux lui-même dispose d'un endroit
où garder le caractère remis ; cela ne peut donc pas avoir d'implémentation par défaut et forme un trait distinct.
`file-stream`/`string-input-stream`/`standard-stream` l'implémentent, et tout autre flux l'obtient une fois enveloppé
avec `make-peek-stream` (chapitre 4).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Fait renvoyer `c` par la prochaine lecture. **La seule méthode à implémenter**. Comme en CL, un seul caractère est garanti |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Regarde le caractère suivant sans le consommer |

De même, pour `CharOutput`, une implémentation n'écrit que `write-item`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Écrit un élément. **La seule méthode à implémenter** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Écrit un caractère |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Écrit une chaîne |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Une chaîne et un saut de ligne |
| `terpri` | `(terpri s)` | `(S)→()` | Un saut de ligne (le nom de CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Un saut de ligne sauf en début de ligne |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Si le prochain caractère écrit commencera une ligne. La valeur par défaut est `false` (`fresh-line` écrit donc le saut de ligne : dans le doute, écrire est le côté sûr). Tous les flux intégrés la redéfinissent |
| `finish-output` | `(finish-output s)` | `(S)→()` | Vide le tampon |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Écrit dans l'ordre tous les caractères de `v` |

`at-line-start` ne se souvient **que de ce qui a été écrit via ce flux**. `print`/`println`/`(format true ...)`
écrivent sur la sortie standard sans passer par `*standard-output*` ; si vous mélangez les deux,
`(fresh-line *standard-output*)` ne connaît donc pas les sauts de ligne écrits par `println`. Tenez-vous-en à l'un
des deux.

`Stream` est commun à tous les flux :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | S'il est encore ouvert |
| `close` | `(close s)` | `(S)→()` | Le ferme. **Le GC ne ferme pas les flux** ; faites-le donc explicitement (ou avec `with-open-file`) |

## 3. Types de flux concrets

| Type | Comment en créer un | Traits implémentés |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` est l'une des trois constantes `direction-input` / `direction-output` / `direction-append`. `open-file`
renvoie `Err(FileError)` si le fichier ne peut pas être ouvert (un fichier manquant est un résultat ordinaire, pas un
panic). Le nom de fichier peut être une chaîne ou un `pathname` (`Pathish` au chapitre 9).

`(get-output-stream-string s)` renvoie ce qui a été écrit dans un `string-output-stream` et le vide. Comme en CL, on
peut l'extraire même après `close`.

**Les E/S d'octets** utilisent `ByteInput`/`ByteOutput`. Ils fixent l'`Item` d'`InputStream`/`OutputStream` à `int`,
de la même façon que `CharInput`/`CharOutput` le fixent à `char`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | L'octet suivant. `none` à la fin du fichier |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Écrit un octet. Une erreur hors de 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | La version caractères, en octets |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Idem |

CL décide du type des éléments **dans l'appel**, comme `(open name :element-type '(unsigned-byte 8))`, mais ici le
type des éléments est **le type** du flux ; ce qui diffère, c'est donc la fonction qui l'ouvre. Lire des octets dans
un flux de caractères est une erreur de type (`string-input-stream` n'implémente pas `ByteInput`). Lire un octet
juste après avoir remis un caractère avec `unread-char` est aussi une erreur.

## 4. Flux composites

Ce sont tous des `defstruct` de la bibliothèque standard, et ils peuvent s'imbriquer.

| Nom | Forme | Description |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Écrit dans tous ceux d'un `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Lit depuis `in` et écrit dans `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Lit depuis `in` et écrit aussi les caractères lus dans `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Lit l'un après l'autre ceux d'un `Vector<:dyn CharInput>` |
| `make-peek-stream` | `(make-peek-stream in)` | Ajoute la remise d'un caractère à n'importe quel `:dyn CharInput`, en faisant un `PeekInput` (pour `read-sexpr`) |

## 5. Macros

| Nom | Forme | Description |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Ouvrir, exécuter le corps, fermer. `Result<valeur du corps, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Lit depuis une chaîne |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Renvoie ce qui a été écrit |

## 6. Fonctions génériques et opérations sur les fichiers

| Nom | Forme | Type | Description |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Transfère tout |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Toutes les lignes restantes |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Lit un `Sexpr` (le `read` de CL). `Ok(eof)` à la fin de l'entrée, `Ok(datum d)` quand une donnée est lue, `Err` si ce n'est pas une donnée. Il **consomme le caractère blanc** qui a terminé la donnée (comme en CL). `ReadOutcome` n'est pas une `Option<Sexpr>` afin que lire la liste vide `()` et la fin de l'entrée ne soient pas la même valeur |
| `read-sexpr-preserving-whitespace` | Idem | Idem | Idem, mais laisse le blanc (le `read-preserving-whitespace` de CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Lit jusqu'à `ch` et en fait une liste. `ch` est consommé. `Err` si l'entrée s'épuise |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Écrit une ligne à la fois |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Tout le contenu |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Toutes les lignes |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | L'écrit |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | S'il existe |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Supprimer, renommer (les arguments sont `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Le chemin absolu avec les liens symboliques et `.`/`..` résolus. `Err` s'il n'existe pas |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | L'heure de la dernière modification. C'est un **temps universel** ; `decode-universal-time` ([Temps](system.md#2-décoder-et-encoder-des-dates)) peut donc le lire |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Le nom de connexion du propriétaire. `Err` si le fichier n'existe pas, `Ok(none)` si l'uid du propriétaire n'a pas d'entrée dans la base des mots de passe : les deux cas que CL distingue restent séparés |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Si c'est un répertoire. **Aussi `false` s'il n'existe pas** ; utilisez `probe-file` pour distinguer les deux |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Liste le contenu par truename (le chemin absolu avec les liens symboliques résolus, comme `truename`). Les liens symboliques dont la cible manque sont omis. `.`/`..` sont omis. L'ordre est celui que donne le système |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Le crée avec ses parents. Réussit s'il existe déjà |

Tout argument désignant un fichier **peut être une chaîne ou un `pathname`**. C'est le même traitement que les
désignateurs de noms de chemin de CL, résolu par le trait `Pathish` plutôt que par un test de type à l'exécution
(chapitre 9).

Le caractère de terminaison de `read-delimited-list` **termine aussi les lexèmes**. Il ne prend effet qu'à la
profondeur 0 : dans `(1 2]`, le `]` est lu comme faisant partie du texte propre de la liste et signalé comme une
liste cassée. Il n'y a pas d'équivalent au troisième argument `recursive-p` de CL.

## 7. Faire de son propre type un flux

Écrivez un seul `write-item` et les implémentations par défaut apportent le reste. Il peut aussi entrer dans des flux
composites.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; toutes les autres méthodes sont celles par défaut

(write-line (counter::new 0) "four")   ; write-line, terpri et fresh-line fonctionnent tous
```

L'entrée fonctionne de la même façon : on n'écrit que `read-item`. Même un type sans remise propre peut être lu une
fois enveloppé, comme dans `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nom | Appel | Type | Description |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` lit le caractère `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Renvoie ce qui est enregistré |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` lit la séquence de deux caractères `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Idem |

`F` est `(fn (string-input-stream char) Option<Sexpr>)`. Comment les utiliser, quand ils prennent effet et en quoi
ils diffèrent de CL se trouve dans la [Référence de la syntaxe](../syntax.md#11-macros-de-lecture-readtable).

## 9. Noms de chemin `pathname`

Un nom de fichier découpé en parties. Il contient les composants de répertoire séparés par `/`, le nom, le type
(l'extension) et s'il commence à la racine.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   coupé au dernier point
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Le trait de désignateur de chemin `Pathish`

Là où CL accepte un désignateur de nom de chemin (une chaîne ou un pathname), ce langage accepte un `Pathish`.
`string` et `pathname` l'implémentent tous deux, et **chaque opération sur les fichiers le prend de façon
générique** ; `(open-input "a.txt")` et `(open-input p)` sont donc tous deux des appels ordinaires (il n'y a pas de
test de type à l'exécution). Le `namestring` d'une chaîne la renvoie simplement ; tant qu'on passe une chaîne,
aucune analyse n'a lieu.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | La forme chaîne. Doit être implémentée |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Convertit en `pathname` (la fonction `pathname` de CL, renommée parce qu'elle entrerait en conflit avec le nom du type). Doit être implémentée |

### 9.2 Fonctions

| Nom | Forme | Type | Description |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Découpe une chaîne en parties. Un `/` final (ou un nom vide) signifie « pas de nom », c'est-à-dire un répertoire |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | En construit un à partir des seuls composants donnés (tous `&key`). Un nom ou un type omis reste « absent » et sera complété par `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Les composants de répertoire, le plus extérieur d'abord |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Le nom sans le type. `none` pour un répertoire |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Ce qui suit le dernier point. Un point initial ne compte pas (tout `.gitignore` est le nom) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | S'il commence à la racine |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Le répertoire personnel. `none` s'il n'y a pas de `$HOME` (CL autorise aussi `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | La partie jusqu'au dernier `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Seulement la partie `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Complète les composants manquants de `p` à partir de `default`. Un `p` relatif va sous le répertoire de `default` ; un `p` absolu garde son propre répertoire |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | La forme relative à `default`. Tout `p` s'il n'est pas sous la base |

Les arguments de type portent tous `(where (Pathish P))`.

## 10. Différences avec CL

- **Une hiérarchie de traits, pas de classes.** Il n'y a pas d'`input-stream-p` / `output-stream-p` : le type porte la
  direction ; ce n'est donc pas une question à poser à l'exécution.
- **`read` a des noms différents pour la version chaîne et la version flux.** `(read "...")` (correspond à la
  première valeur du `read-from-string` de CL ; si vous avez aussi besoin de la position où la lecture s'est
  arrêtée, utilisez `read-from-string`) et `(read-sexpr s)` (le `read` de CL). Un appel se résout vers un seul type
  de receveur ; un même nom ne peut donc pas être surchargé.
- **La remise est un trait distinct** (`PeekInput`) ; les types qui n'ont besoin que de `read-char` ne sont donc pas
  obligés d'implémenter `unread-char`.
- **La fermeture est explicite.** Le GC ne ferme pas les flux (le GC s'exécute à des moments imprévisibles ; lui
  laisser la fermeture la rendrait aussi imprévisible). Utiliser `with-open-file` est la voie sûre.
- **Les noms de chemin n'ont pas de composants hôte, périphérique ou version.** Il n'y a ni noms de chemin à jokers
  ni noms de chemin logiques (`logical-pathname`). Le séparateur est toujours `/`.
- **La fonction `pathname` s'appelle `to-pathname`**, parce que types, traits et fonctions partagent un même espace
  de noms.
- **Il n'y a pas de correspondance par jokers** ; `directory` est donc une fonction qui « liste le contenu de ce
  répertoire », rien de plus. Le `directory` de CL fait correspondre un motif de nom de chemin.

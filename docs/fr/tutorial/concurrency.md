<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Concurrence

En typelisp, on exécute du travail de façon concurrente en lançant des **tâches** (des threads légers), et les
tâches se passent des valeurs par des **canaux**. Le modèle est proche des goroutines et des canaux de Go. Ce
chapitre aborde dans l'ordre le lancement d'une tâche et la récupération de son résultat, les canaux, `select`, la
protection des données partagées et les threads système dédiés. Il suppose que vous avez lu
[Bases des types](types.md).

## 1. Lancer une tâche et attendre son résultat

`(task (fonction arguments...))` lance un appel de fonction comme nouvelle tâche. Le côté qui lance n'attend pas et
continue. La valeur est une poignée de type `Task<T>` ; `(wait poignée)` attend la fin de la tâche et renvoie son
résultat.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; attendre 0,1 seconde (seule cette tâche s'arrête)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` ne prend que la forme d'un appel de fonction. Les arguments sont évalués là où le `task` est écrit ;
  seul l'appel lui-même s'exécute dans la nouvelle tâche.
- Pour exécuter plusieurs expressions, créez un `lambda` et appelez-le sur place :
  `(task ((lambda () () (println "start") (work))))`
- On peut appeler `wait` autant de fois qu'on veut. Le résultat est mémorisé.
- Une tâche s'exécute même si on ne l'attend jamais avec `wait`.
- **Quand le travail principal se termine, le programme se termine.** Les tâches encore en cours sont
  interrompues.

## 2. Passer des valeurs par des canaux

Un canal `Chan<T>` est un chemin par lequel les tâches se passent des valeurs de type `T`. L'argument de
`Chan::new` est la capacité (combien de valeurs il peut contenir). Sur un canal de capacité 0, l'émetteur et le
récepteur attendent tous deux que l'autre côté soit là.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; envoyer
  (close ch))                  ; plus d'envois

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; recevoir jusqu'à la fermeture
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` envoie. Si le canal est plein, il attend qu'il y ait de la place.
- `(recv ch)` reçoit. Il attend qu'une valeur arrive. Le résultat est une `Option<T>` ; une fois le canal fermé
  et vide, il renvoie `none`.
- Parcourir un canal avec `doiter` continue de recevoir des valeurs jusqu'à sa fermeture. On peut aussi le passer
  directement à `map` ou `filter`.
- `send` sur un canal fermé déclenche un panic.

### Répartir le travail entre plusieurs tâches

Un schéma courant consiste à mettre en place un canal qui transporte le travail et à faire prendre les tâches à
plusieurs workers (des tâches qui traitent le travail). Le worker libre prend le travail suivant ; ainsi, même
quand des travaux lents et rapides sont mélangés, le travail se répartit naturellement.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; durée de ce travail

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; prendre un travail à la fois jusqu'à la fermeture de jobs
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; au travail ; pendant ce temps, les autres workers prennent les travaux suivants
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; combien de travaux ce worker a faits

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; c'est tout le travail
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- A, B et C prennent chacun l'un des trois premiers travaux.
- Au bout de 0,1 seconde, B et C sont libres et prennent les travaux restants. Tant que A est occupé par le travail
  lent 1, il ne prend pas de nouveau travail.
- À la fin, A a traité un travail, B deux et C trois. Rien dans le programme ne dit quel worker prend quel travail.
- Fermer `jobs` termine le `doiter` de chaque worker, les tâches se terminent et chaque `wait` renvoie son compte.

`jobs` est un canal de capacité 0 ; `send` attend donc qu'un worker prenne le travail. Avec une capacité plus
grande, la tâche principale pourrait mettre du travail en file sans attendre les workers.

## 3. `select` : attendre plusieurs canaux à la fois

`select` effectue celle de plusieurs opérations de canal qui devient possible en premier. `(after secondes)` est
un canal qui livre une valeur une fois le temps donné écoulé. Combiné avec `select`, il donne un délai d'attente.

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) body...)` est une branche de réception. `v` reçoit une `Option<T>`.
- `((send ch x) body...)` est une branche d'envoi.
- Quand plusieurs branches peuvent avancer en même temps, l'une d'elles est choisie au hasard.
- Avec `(else body...)` à la fin, `else` s'exécute quand aucune branche ne peut avancer immédiatement, et `select`
  n'attend pas.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Protéger les données partagées

Quand plusieurs tâches modifient la même valeur, protégez-la avec `Mutex<T>`. `with-lock` prend le verrou, lie le
contenu à une variable, exécute le corps et libère toujours le verrou, quelle que soit la façon dont le corps est
quitté. Affecter la variable avec `setf` dans le corps change le contenu du `Mutex`.

`WaitGroup` est un outil pour attendre qu'un nombre donné de tâches soient terminées. Augmentez le compteur avec
`add`, faites appeler `done` à chaque tâche quand elle se termine, et attendez avec `wait` que le compteur atteigne
0.

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

Si plusieurs tâches modifient la même valeur en même temps sans passer par un `Mutex` ou un canal, le résultat
n'est pas garanti. Passez les données entre tâches par des canaux quand c'est possible, et ne partagez des
données que lorsque c'est nécessaire.

## 5. Où les tâches changent

Les tâches changent de façon coopérative. Une tâche ne cède la place aux autres qu'à ces endroits :

- `(yield)`, `(sleep secondes)`, `(wait poignée)`
- Une opération de canal qui doit attendre (`send`, `recv`, `select`)
- Une opération sur socket qui doit attendre (connexion, lecture, écriture, etc.)

L'argument de `sleep` est un nombre de secondes de type `f64`. Écrivez `(sleep 1.0)`, pas `(sleep 1)`.

Les tâches s'exécutent en même temps sur plusieurs threads système. Cependant, quand `typl` exécute un programme
directement, seules les tâches qui exécutent des fonctions [compilées](../guide/compile.md) partent sur d'autres
threads. Les autres tâches s'exécutent sur un seul thread, en changeant aux endroits ci-dessus.

## 6. `thread` : s'exécuter sur un thread système dédié

Un travail qui ne doit pas retarder les autres tâches, comme l'appel d'une fonction C lente
([FFI C](../guide/ffi.md)), se lance avec `thread`. Il s'écrit comme `task` et obtient son propre thread système.
On attend sa fin avec `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- La poignée de `thread` a le type `Thread<T>`. Comme `wait`, `join` peut être appelé autant de fois qu'on veut.
- `thread` ne peut exécuter que des fonctions compilables. Quand on exécute dans `typl`, la fonction qu'il appelle
  est compilée sur place avant de s'exécuter.

## 7. Les tâches et les autres fonctionnalités

- Un `panic` dans une tâche arrête tout le programme.
- `throw` n'atteint pas l'extérieur d'une tâche. Un `throw` qui sortirait du corps de la tâche devient un `panic`.
- La sortie d'un `println` n'est jamais mêlée au milieu d'une ligne avec la sortie d'autres tâches.

## 8. Que lire ensuite

- [Tâches et canaux](../reference/functions/concurrency.md) : la liste des fonctions
- [Chapitre 12 de la référence de la syntaxe](../reference/syntax.md#12-concurrence-tâches) : où les tâches
  changent, en détail, et les différences avec Go
- [E/S de fichiers, flux et réseau](../guide/io.md) : écrire un serveur avec des tâches

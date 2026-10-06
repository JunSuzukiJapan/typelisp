<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Tâches et canaux

Le vocabulaire des tâches (threads légers). `task` et `thread`, qui les lancent, et `select`, qui attend plusieurs
choses, sont des formes spéciales et se trouvent dans la
[Référence de la syntaxe](../syntax.md#12-concurrence-tâches). Ce chapitre couvre le reste : types, méthodes et
fonctions.

Les tâches sont **coopératives** : une tâche ne change qu'aux endroits que vous écrivez. Les tâches s'exécutent en
même temps sur `TYPELISP_THREADS` threads système (dans `typl`, seules les tâches compilées partent sur d'autres
threads). Où elles changent et en quoi cela diffère de Go se trouve dans
[Référence de la syntaxe 12.5](../syntax.md#125-où-les-tâches-changent) et
[12.7](../syntax.md#127-différences-avec-go).

## 1. `Task<T>` — poignées de tâches

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Attend la fin et renvoie sa valeur |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **On peut appeler `wait` autant de fois qu'on veut** (la valeur est mise en cache). Contrairement au
  `JoinHandle::join` de Rust, il ne consomme pas la poignée ; on peut donc attendre depuis plusieurs endroits.
- **Une tâche s'exécute même si on ne fait jamais `wait`.** Abandonner la poignée ne l'arrête pas.
- C'est une valeur ordinaire ; elle peut donc aller dans un `Vector<Task<()>>`.
- **Quand la tâche principale se termine, le processus se termine** (comme en Go). Les autres tâches en cours sont
  interrompues, et le nettoyage de `unwind-protect` ne s'exécute pas, car il s'agit d'une fin de processus, pas d'un
  déroulement de pile.

## 2. `Chan<T>` — canaux

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Un canal de capacité `n`. `0` est un rendez-vous (sans tampon) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Attend qu'il y ait de la place, puis transmet la valeur |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Attend qu'une valeur arrive. `none` une fois le canal fermé et vide |
| `close` | `(close ch)` | `(Chan<T>)→()` | Le ferme |
| `len` | `(len ch)` | `(Chan<T>)→int` | Combien de valeurs sont actuellement dans le tampon |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | La capacité |

**L'argument de type se donne avec `the`** (écrit de la même façon que `(the Vector<i32> (Vector::new))`). **La
capacité doit toujours être écrite** : les deux cas que Go écrit `make(chan int)` et `make(chan int, 16)`
s'écrivent `(Chan::new 0)` et `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; le for v := range ch de Go
```

- **Un `Chan<T>` est son propre itérateur** (il implémente `Iter`). `recv` renvoie une `Option<T>`, le même type
  que `Iter::next` ; `doiter` et `map`/`filter`/`foldl` fonctionnent donc tels quels dessus.
- **`send` sur un canal fermé déclenche un panic**, et **un second `close` aussi** (les deux comme en Go). Ce sont
  des bogues du programme, pas des échecs récupérables ; ce ne sont donc pas des `Result`.
- **Fermer un canal sur lequel une tâche attend pour faire `send` fait déclencher un panic à cette tâche** (la
  règle de Go).
- `recv` sur un canal fermé renvoie ce qui reste dans le tampon, puis, une fois vide, continue de renvoyer `none`.
- `close` est résolu selon le type du receveur ; c'est donc autre chose que le `close` du trait `Stream`.
  `Chan<T>` n'implémente pas `Stream`.
- **Une capacité négative déclenche un panic** (elle n'est pas silencieusement arrondie à 0).

## 3. `yield` / `sleep` — céder la place

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Cède le reste de son tour (le `runtime.Gosched` de Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | N'arrête **que cette tâche**. Les autres continuent |

`sleep` arrête une tâche, pas un thread. C'est seulement quand aucune tâche ne peut s'exécuter qu'il passe à un
`sleep` du système jusqu'à l'échéance la plus proche. `(sleep 0.0)` est le « céder pendant 0 seconde » de CL.

Comme en CL, `sleep` prend des **secondes**. Les entiers ne sont pas convertis automatiquement en nombres à virgule
flottante ; le `(sleep 1)` de CL s'écrit donc ici `(sleep 1.0)`. Une valeur négative ou NaN déclenche un panic.

## 4. `WaitGroup` — attendre N achèvements

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Un groupe sans rien en attente |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Ajoute au compteur. À faire avant le début du travail |
| `done` | `(done wg)` | `(WaitGroup)→()` | Une chose est terminée. À 0, tous ceux qui attendent sont libérés |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Attend que le compteur atteigne 0. Depuis un nombre quelconque de tâches |

`(wait wg)` et `(wait task)` sont résolus selon le type du receveur ; ils coexistent donc sous un même nom. Si le
compteur passe sous 0, c'est un panic (`done` appelé trop souvent, ou un `add` négatif). Comme en Go, un groupe
revenu à 0 peut resservir en commençant par `add`. Aucune mise à jour n'est perdue, même quand les tâches
s'exécutent sur des threads système différents.

**Avec `Task<T>`, on en a moins besoin qu'en Go** : `(doiter (t tasks) (wait t))` suffit souvent. C'est un outil
pour un travail qui croît dynamiquement, ou quand on ne veut pas conserver les poignées.

```lisp
;; fan-in : lancer une tâche par entrée et les réunir (ce langage n'a pas de canaux nil)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — un canal qui livre après un délai

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Un canal qui livre une valeur après `sec` secondes |

Le `time.After` de Go. Il s'écrit tel quel dans la branche de délai d'un `select`
([Référence de la syntaxe 12.3](../syntax.md#123-select--attendre-plusieurs-opérations-de-canal-à-la-fois)). Sa
capacité est 1 ; la tâche émettrice peut donc se terminer même si personne ne reçoit.

## 6. `Mutex<T>` — exclusion mutuelle pour les données partagées

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Un mutex non verrouillé contenant `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Prend le verrou (attend) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Le libère. Panic s'il n'est pas verrouillé |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Verrouille, lie le contenu à `x`, exécute `body` et libère **toujours** |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` n'est pas une copie de la valeur mais un « emplacement »** (`symbol-macrolet`). `(setf x 42)` change le
  contenu du mutex.
- `with-lock` libère avec `unwind-protect` ; le verrou est donc libéré quelle que soit la façon dont le corps est
  quitté : fin normale, `throw`, `panic` ou `break`/`return`/`return-from`.
- **Y rentrer de nouveau provoque un interblocage** (pas de panic). L'ordonnanceur signale que « rien ne peut
  progresser » pour une tâche bloquée sur son propre verrou.
- **`m::v` touche le contenu en dehors du verrou**, ce qui est indéfini au sens où une autre tâche peut être en
  train de le modifier. C'est la même position que le `sync.Mutex` de Go : dans un langage sans vérification de
  propriété ni d'emprunt, une garantie statique comme `MutexGuard` ne peut pas être construite.

## 7. `Thread<T>` — threads système dédiés

La poignée renvoyée par `(thread (f args...))`
([Référence de la syntaxe 12.2](../syntax.md#122-thread--lancer-une-tâche-sur-un-thread-système-dédié)). Le
pendant de `Task<T>`.

| Nom | Usage | Type | Signification |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Attend la fin et renvoie la valeur (la **tâche** appelante s'arrête. Peut être appelé autant de fois qu'on veut ; la valeur est mise en cache) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | La version fonction de `(thread (f))` (le `std::thread::spawn` de Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Le numéro du thread système en cours d'exécution. Unique dans le processus, sans autre signification que « est-ce le même thread » |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Le nombre de threads que la machine peut exécuter à la fois (la valeur par défaut de `TYPELISP_THREADS`). Panic si le système ne répond pas |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; une fonction C bloquante
(let ((th (thread (sleepy 500000))))
  ...                                            ; les autres tâches continuent pendant ce temps
  (join th))                                     ; => 500000
```

- Appeler une fonction C bloquante (`defffi`) n'arrête que ce thread.
- Un `task` à l'intérieur d'un `thread` s'exécute comme une tâche ordinaire sur d'autres threads.
- Utilisable aussi dans `typl`. En interprétation, `(thread (f ...))` compile `f` sur place puis l'exécute sur le
  thread dédié.
- `Thread::spawn` prend une valeur fonctionnelle et ne compile donc pas sur place. L'appeler depuis un niveau
  supérieur que `typl` interprète déclenche un panic
  ([Référence de la syntaxe 12.2](../syntax.md#122-thread--lancer-une-tâche-sur-un-thread-système-dédié)). Il est
  utilisable depuis des fonctions compilées.

## 8. Ce qui n'existe pas

- **`Atomic`**. `Mutex` suffit.
- **Variables locales aux tâches** (Go n'en a pas non plus).
- **Canaux nil**. La raison et l'alternative se trouvent dans
  [Référence de la syntaxe 12.7](../syntax.md#127-différences-avec-go).

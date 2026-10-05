<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Concurrencia

En typelisp el trabajo concurrente se hace iniciando **tareas** (hilos ligeros), y las tareas se pasan
valores entre sí a través de **canales**. El modelo se parece a las goroutines y los canales de Go. Este
capítulo trata, por orden, cómo iniciar una tarea y obtener su resultado, los canales, `select`, la
protección de datos compartidos y los hilos de SO dedicados. Se supone que has leído
[Fundamentos de tipos](types.md).

## 1. Iniciar una tarea y esperar su resultado

`(task (función argumentos...))` inicia una llamada a función como una tarea nueva. Quien la inicia no
espera y continúa. El valor es un manejador de tipo `Task<T>`; `(wait manejador)` espera a que la tarea
termine y devuelve su resultado.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; espera 0,1 segundos (solo se detiene esta tarea)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` solo acepta la forma de una llamada a función. Los argumentos se evalúan donde se escribe el
  `task`; en la tarea nueva solo se ejecuta la llamada.
- Para ejecutar varias expresiones, crea una `lambda` y llámala en el acto:
  `(task ((lambda () () (println "start") (work))))`
- Puedes llamar a `wait` tantas veces como quieras. El resultado se recuerda.
- Una tarea se ejecuta aunque nunca hagas `wait` sobre ella.
- **Cuando termina el trabajo principal, el programa termina.** Las tareas que sigan en marcha se cortan.

## 2. Pasar valores por canales

Un canal `Chan<T>` es una vía por la que las tareas se pasan valores de tipo `T`. El argumento de
`Chan::new` es la capacidad (cuántos valores puede guardar). En un canal de capacidad 0, el emisor y el
receptor esperan los dos hasta que el otro lado está presente.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; enviar
  (close ch))                  ; no habrá más envíos

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; recibir hasta que se cierre
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` envía. Si el canal está lleno, espera hasta que haya sitio.
- `(recv ch)` recibe. Espera hasta que llega un valor. El resultado es un `Option<T>`; una vez que el canal
  está cerrado y vacío, devuelve `none`.
- Recorrer un canal con `doiter` sigue recibiendo valores hasta que se cierra. También se puede pasar
  directamente a `map` o `filter`.
- `send` sobre un canal cerrado provoca un panic.

### Repartir el trabajo entre varias tareas

Un patrón habitual es preparar un canal que transporta el trabajo y hacer que varios trabajadores (tareas
que procesan el trabajo) tomen tareas de él. El trabajador que esté libre toma el siguiente trabajo, así
que aunque se mezclen trabajos lentos y rápidos, el trabajo se reparte de forma natural.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; cuánto tarda este trabajo

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; toma un trabajo cada vez hasta que se cierre jobs
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; trabajando; mientras, otros trabajadores toman los siguientes
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; cuántos trabajos hizo este trabajador

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
  (close jobs)                    ; ese es todo el trabajo
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

- A, B y C toman cada uno uno de los tres primeros trabajos.
- A los 0,1 segundos B y C quedan libres y toman los trabajos restantes. Mientras A está ocupado con el
  lento trabajo 1, no toma trabajo nuevo.
- Al final A se ocupó de un trabajo, B de dos y C de tres. Nada en el programa dice qué trabajador toma
  qué trabajo.
- Cerrar `jobs` termina el `doiter` de cada trabajador, las tareas terminan y cada `wait` devuelve su
  recuento.

`jobs` es un canal de capacidad 0, así que `send` espera hasta que algún trabajador toma el trabajo. Con una
capacidad mayor, la tarea principal podría encolar trabajo sin esperar a los trabajadores.

## 3. `select`: esperar varios canales a la vez

`select` realiza la primera de varias operaciones de canal que se vuelva posible. `(after segundos)` es un
canal que entrega un valor una vez transcurrido el tiempo dado. Combinado con `select`, da un tiempo de
espera máximo.

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

- `((v (recv ch)) cuerpo...)` es una rama de recepción. `v` recibe un `Option<T>`.
- `((send ch x) cuerpo...)` es una rama de envío.
- Cuando varias ramas pueden avanzar a la vez, se elige una al azar.
- Con `(else cuerpo...)` al final, `else` se ejecuta cuando ninguna rama puede avanzar de inmediato, y
  `select` no espera.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Proteger datos compartidos

Cuando varias tareas modifican el mismo valor, protégelo con `Mutex<T>`. `with-lock` toma el cerrojo,
liga el contenido a una variable, ejecuta el cuerpo y libera siempre el cerrojo se salga como se salga del
cuerpo. Asignar a la variable con `setf` dentro del cuerpo cambia el contenido del `Mutex`.

`WaitGroup` es una herramienta para esperar a que termine un número dado de tareas. Sube la cuenta con
`add`, haz que cada tarea llame a `done` al terminar, y haz `wait` hasta que la cuenta llegue a 0.

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

Si varias tareas modifican el mismo valor a la vez sin pasar por un `Mutex` o un canal, el resultado no
está garantizado. Pasa los datos entre tareas por canales cuando puedas, y comparte datos solo cuando lo
necesites.

## 5. Dónde cambian las tareas

Las tareas se alternan de forma cooperativa. Una tarea cede el paso a otras solo en estos puntos:

- `(yield)`, `(sleep segundos)`, `(wait manejador)`
- Una operación de canal que tiene que esperar (`send`, `recv`, `select`)
- Una operación de socket que tiene que esperar (conectar, leer, escribir, etc.)

El argumento de `sleep` es un número `f64` de segundos. Escribe `(sleep 1.0)`, no `(sleep 1)`.

Las tareas se ejecutan a la vez en varios hilos de SO. Sin embargo, cuando `typl` ejecuta un programa
directamente, solo las tareas que ejecutan funciones [compiladas](../guide/compile.md) salen a otros
hilos. Las demás tareas se ejecutan en un único hilo, alternándose en los puntos de arriba.

## 6. `thread`: ejecutar en un hilo de SO dedicado

El trabajo que no debe frenar a otras tareas, como llamar a una función de C lenta
([FFI de C](../guide/ffi.md)), se inicia con `thread`. Se escribe igual que `task` y obtiene un hilo de SO
propio. Espera a que termine con `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- El manejador de `thread` tiene tipo `Thread<T>`. Como `wait`, `join` se puede llamar cualquier número de
  veces.
- `thread` solo puede ejecutar funciones que se puedan compilar. Al ejecutar en `typl`, la función que
  llama se compila en el acto antes de ejecutarse.

## 7. Las tareas y otras funciones del lenguaje

- Un `panic` dentro de una tarea detiene el programa entero.
- `throw` no llega fuera de una tarea. Un `throw` que saldría del cuerpo de la tarea se convierte en un
  `panic`.
- La salida de un `println` nunca se mezcla a mitad de línea con la salida de otras tareas.

## 8. Qué leer después

- [Tareas y canales](../reference/functions/concurrency.md): la lista de funciones
- [Referencia de sintaxis capítulo 12](../reference/syntax.md#12-concurrencia-tareas): dónde cambian las
  tareas en detalle, y diferencias con Go
- [E/S de archivos, streams y red](../guide/io.md): escribir un servidor con tareas

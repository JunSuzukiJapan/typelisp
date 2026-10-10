<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Tareas y canales

El vocabulario de las tareas (hilos ligeros). `task` y `thread`, que las inician, y `select`, que espera
varias cosas, son formas especiales y están en la [Referencia de sintaxis](../syntax.md#12-concurrencia-tareas).
Este capítulo trata el resto: tipos, métodos y funciones.

Las tareas son **cooperativas**: una tarea solo cambia en los puntos que escribes. Las tareas se ejecutan a
la vez en `TYPELISP_THREADS` hilos de SO (en `typl`, solo las tareas compiladas salen a otros hilos). Dónde
cambian y en qué se diferencia de Go está en
[Referencia de sintaxis 12.5](../syntax.md#125-dónde-cambian-las-tareas) y
[12.7](../syntax.md#127-diferencias-con-go).

## 1. `Task<T>` — manejadores de tareas

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Espera a que termine y devuelve su valor |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Puedes hacer `wait` cualquier número de veces** (el valor se guarda en caché). A diferencia de
  `JoinHandle::join` de Rust, no consume el manejador, así que se puede esperar desde varios lugares.
- **Una tarea se ejecuta aunque nunca hagas `wait`.** Descartar el manejador no la detiene.
- Es un valor normal, así que puede ir en un `Vector<Task<()>>`.
- **Cuando termina la tarea principal, termina el proceso** (como en Go). Las demás tareas en marcha se
  cortan, y la limpieza de `unwind-protect` no se ejecuta, porque esto es la salida del proceso, no un
  desenrollado de la pila.

## 2. `Chan<T>` — canales

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Un canal de capacidad `n`. `0` es un encuentro (sin búfer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Espera hasta que haya sitio y entrega el valor |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Espera hasta que llega un valor. `none` una vez cerrado y vacío |
| `close` | `(close ch)` | `(Chan<T>)→()` | Lo cierra |
| `len` | `(len ch)` | `(Chan<T>)→int` | Cuántos valores hay ahora en el búfer |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | La capacidad |

**El argumento de tipo se da con `the`** (igual que `(the Vector<i32> (Vector::new))`). **La capacidad hay
que escribirla siempre**: los dos casos que Go escribe como `make(chan int)` y `make(chan int, 16)` se
escriben `(Chan::new 0)` y `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; el for v := range ch de Go
```

- **Un `Chan<T>` es su propio iterador** (implementa `Iter`). `recv` devuelve un `Option<T>`, el mismo tipo
  que `Iter::next`, así que `doiter` y `map`/`filter`/`foldl` funcionan sobre él tal cual.
- **`send` sobre un canal cerrado provoca un panic**, y **un segundo `close` también** (ambos como en Go).
  Son errores del programa, no fallos recuperables, así que no son `Result`.
- **Cerrar un canal en el que una tarea espera para hacer `send` provoca un panic en esa tarea** (la regla de
  Go).
- `recv` sobre un canal cerrado devuelve lo que queda en el búfer y, una vez vacío, sigue devolviendo
  `none`.
- `close` se resuelve por el tipo del receptor, así que es algo distinto del `close` del trait `Stream`.
  `Chan<T>` no implementa `Stream`.
- **Una capacidad negativa provoca un panic** (no se redondea en silencio a 0).

## 3. `yield` / `sleep` — ceder el turno

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Cede el resto de su turno (el `runtime.Gosched` de Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Detiene **solo esa tarea**. Las demás siguen en marcha |

`sleep` detiene una tarea, no un hilo. Solo cuando no puede ejecutarse ninguna tarea entra en un `sleep`
del SO hasta el plazo más cercano. `(sleep 0.0)` es el "ceder durante 0 segundos" de CL.

Como en CL, `sleep` recibe **segundos**. Los enteros no se convierten automáticamente en números de coma
flotante, así que el `(sleep 1)` de CL se escribe aquí `(sleep 1.0)`. Un valor negativo o NaN provoca un
panic.

## 4. `WaitGroup` — esperar N finalizaciones

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Un grupo sin nada pendiente |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Suma al contador. Hazlo antes de que empiece el trabajo |
| `done` | `(done wg)` | `(WaitGroup)→()` | Ha terminado uno. Al llegar a 0, se libera a todos los que esperan |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Espera hasta que llegue a 0. Desde cualquier número de tareas |

`(wait wg)` y `(wait task)` se resuelven por el tipo del receptor, así que conviven con un mismo nombre. Si
el contador baja de 0, provoca un panic (`done` llamado demasiadas veces, o un `add` negativo). Como en Go,
un grupo que ha vuelto a 0 se puede volver a usar empezando por `add`. No se pierde ninguna actualización
aunque las tareas se ejecuten en hilos de SO distintos.

**Con `Task<T>` disponible, se necesita menos que en Go**: `(doiter (t tasks) (wait t))` suele bastar. Es una
herramienta para trabajo que crece dinámicamente, o para cuando no quieres guardar los manejadores.

```lisp
;; fan-in: iniciar una tarea por entrada y reunirlas (este lenguaje no tiene canales nil)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — un canal que entrega tras un tiempo

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Un canal que entrega un valor pasados `sec` segundos |

El `time.After` de Go. Se puede escribir tal cual en la rama de tiempo de espera de `select`
([Referencia de sintaxis 12.3](../syntax.md#123-select--esperar-varias-operaciones-de-canal-a-la-vez)). Su
capacidad es 1, así que la tarea que envía puede terminar aunque nadie reciba.

## 6. `Mutex<T>` — exclusión mutua para datos compartidos

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Un mutex sin bloquear que contiene `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Toma el cerrojo (espera) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Lo libera. Panic si no está bloqueado |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Bloquea, liga el contenido a `x`, ejecuta `body` y **siempre** libera |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` no es una copia del valor sino un "lugar"** (`symbol-macrolet`). `(setf x 42)` cambia el contenido
  del mutex.
- `with-lock` libera con `unwind-protect`, así que el cerrojo se libera se salga como se salga del cuerpo:
  terminación normal, `throw`, `panic` o `break`/`return`/`return-from`.
- **Volver a entrar produce un interbloqueo** (no un panic). El planificador informa de que "nada puede
  avanzar" para una tarea atascada en su propio cerrojo.
- **`m::v` toca el contenido desde fuera del cerrojo**, lo cual está indefinido en el sentido de que otra
  tarea puede estar cambiándolo. Es la misma postura que el `sync.Mutex` de Go: en un lenguaje sin
  propiedad ni comprobación de préstamos, no se puede construir una garantía estática como `MutexGuard`.

## 7. `Thread<T>` — hilos de SO dedicados

El manejador que devuelve `(thread (f args...))`
([Referencia de sintaxis 12.2](../syntax.md#122-thread--iniciar-una-tarea-en-un-hilo-de-so-dedicado)). La
contrapartida de `Task<T>`.

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Espera a que termine y devuelve el valor (se detiene la **tarea** que llama. Se puede llamar cualquier número de veces; el valor se guarda en caché) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | La versión en forma de función de `(thread (f))` (el `std::thread::spawn` de Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | El número del hilo de SO que se está ejecutando. Único dentro del proceso, sin más significado que "¿es el mismo hilo?" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | El número de hilos que la máquina puede ejecutar a la vez (el valor por defecto de `TYPELISP_THREADS`). Panic si el SO no responde |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; una función de C bloqueante
(let ((th (thread (sleepy 500000))))
  ...                                            ; mientras, las demás tareas siguen avanzando
  (join th))                                     ; => 500000
```

- Llamar a una función de C bloqueante (`defffi`) detiene solo ese hilo.
- Un `task` dentro de un `thread` se ejecuta como una tarea normal en otros hilos.
- También se puede usar en `typl`. Al interpretar, `(thread (f ...))` y `Thread::spawn` compilan en el acto la
  función que se va a ejecutar y después la ejecutan en el hilo dedicado. Una `lambda` que hace referencia a
  variables locales de fuera no se puede compilar por sí sola y provoca un panic
  ([Referencia de sintaxis 12.2](../syntax.md#122-thread--iniciar-una-tarea-en-un-hilo-de-so-dedicado)). Una `lambda` creada
  dentro de una función compilada sí se puede pasar.

## 8. `Context` — cancelación cooperativa

El `context.Context` de Go. Se pasa a un trabajo que se quiere poder detener desde fuera. Detenerlo
es **cooperativo**: `cancel` no interrumpe nada; una tarea o un hilo se entera comprobando por sí
mismo `is-cancelled` o recibiendo de `done`.

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | Un contexto nuevo que sirve de raíz |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | Crea un hijo de `parent` |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | Crea un hijo de `parent` que se cancela solo al cabo de `sec` segundos |
| `cancel` | `(cancel ctx)` | `(Context)→()` | Cancela. Se puede llamar cuantas veces se quiera |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | Un canal que se cierra cuando se cancela el contexto |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | Si se ha cancelado |

```lisp
(defun worker ((ctx Context) (jobs Chan<int>)) ()
  (loop
    (select
      ((v (recv (done ctx))) (println "stopped") (break))
      ((j (recv jobs)) (match j
                         ((some n) (println "job ~a" n))
                         ((none) (break)))))))

(let* ((ctx (Context::with-timeout (Context::background) 1.0))
       (jobs (the Chan<int> (Chan::new 0))))
  (task (worker ctx jobs))
  (send jobs 1)
  (send jobs 2)
  (cancel ctx)                          ; job 1, job 2 y luego stopped
  (sleep 0.1))
```

- **La cancelación llega a los hijos.** Un contexto creado con `with-cancel`/`with-timeout` se
  cancela junto con su padre. En sentido contrario (de hijo a padre) no se propaga.
- Un hijo creado a partir de un contexto ya cancelado nace cancelado.
- `done` solo se cierra; no se envía ningún valor. Recibir devuelve `none`.
- Cada llamada a `(Context::background)` crea una raíz distinta. El `Background()` de Go es uno solo
  y no se puede cancelar; aquí también una raíz se puede cancelar, y eso solo afecta a lo creado a
  partir de ella.
- Un contexto se puede pasar entre tareas y entre hilos.

## 9. Lo que no hay

- **`Atomic`**. `Mutex` basta.
- **Variables locales de tarea** (Go tampoco las tiene).
- **Canales nil**. La razón y la alternativa están en
  [Referencia de sintaxis 12.7](../syntax.md#127-diferencias-con-go).

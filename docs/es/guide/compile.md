<!-- translated-from: docs/ja/guide/compile.md @ 3a77c293e452d2d74bccfa67efd8af1f6f650f7d -->
# Compilación

Si no se hace nada más, los programas de typelisp se ejecutan en el intérprete. Además hay dos formas de
compilar a código nativo y una forma de guardar un entorno. Los detalles de la especificación están en
el [capítulo 10 de la Referencia de sintaxis](../reference/syntax.md#10-compilación).

| Método | Cómo | Resultado |
|---|---|---|
| Compilación JIT | `(compile name)` | Una función de la sesión en curso se sustituye por código nativo |
| Compilación AOT | `typl -c src.typl` o `(compile-file "src.typl" "out")` | Un ejecutable independiente |
| Volcado | `(dump "file.typld")` | Guarda las definiciones; `typl --image` vuelve a arrancar desde el mismo entorno |

## 1. Preparación

La compilación usa LLVM 22. Si has compilado `typl` siguiendo el [README.md](../../../README.md), no hace
falta más preparación.

Los ejecutables creados por compilación AOT se enlazan con la biblioteca estática `libtypelisp_front.a`. Una
compilación de release de `typl` (incluida la instalada con `cargo install`) lleva esta biblioteca dentro,
así que no hace falta preparar nada. La primera vez que compila, escribe la biblioteca en
`~/.typelisp/lib/<ID de compilación>/` (o en `$TYPELISP_HOME/lib/<ID de compilación>/` si está definida la
variable de entorno `TYPELISP_HOME`) y usa esa copia a partir de entonces. `typl --remove-lib` la borra
(con `--others`, las escritas por otras versiones de `typl`; con `--all`, todas). Una compilación de
depuración de `typl` usa la biblioteca de `target/debug/` del repositorio donde se compiló. Para usar una
colocada en otro sitio, indica su carpeta con `--lib-dir` al arrancar `typl` (sección 3.2).
En macOS, el enlazado usa las Xcode Command Line Tools.

## 2. Compilación JIT

Convierte en el acto una función ya definida en código nativo.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; a partir de aquí, las llamadas ejecutan el código compilado
```

- `name` no se evalúa. Escribe el nombre de la función tal cual (no como cadena). Para un método, escríbelo
  con el nombre del tipo, como en `(compile point::norm)`.
- Las funciones a las que llama se compilan con ella.
- **Las funciones genéricas no se pueden compilar.** En cada lugar donde se usan se crea una copia para
  cada tipo. Compila en su lugar la función que la llama con tipos concretos.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` y `dump` son operaciones del intérprete, así que
  una función que las llama no se puede compilar. Intentar compilarla da un error que explica por qué.

Para ver el resultado de la compilación, usa `disassemble`.

```lisp
(disassemble fib)          ; el código máquina del anfitrión
(disassemble fib true)     ; LLVM IR
```

## 3. Construir un ejecutable con compilación AOT

### 3.1 Escribir el programa

Como punto de entrada, define una **función `main` que no recibe argumentos**.

```lisp
(use std::env::command-line-args)
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

El `(main)` al final del archivo está ahí para que se llame a `main` al ejecutar `typl hello.typl`.
`compile-file` se salta este `(main)` final, así que el mismo archivo sirve tanto en el intérprete como con
la compilación AOT.

### 3.2 Compilar

Desde la línea de órdenes, usa `typl -c` (`typl --compile` es lo mismo).

```sh
$ typl -c hello.typl            # crea hello
$ typl -c hello.typl -o fib     # llama fib al ejecutable
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Sin `-o`, el ejecutable toma el nombre del archivo fuente sin `.typl` y se coloca en la misma carpeta que
el archivo fuente. Si el nombre del archivo fuente no termina en `.typl`, `-o` es obligatorio. Con `-c`
(`--compile`) no se pueden dar `--image` ni `--feature`.

Con `--release`, `typl` hace una **compilación release**, que elimina `eprint`/`eprintln` (salida de error estándar para investigar; [Impresión](../reference/functions/printing.md)) junto con sus argumentos. Lo que elimina se sigue comprobando por tipos, así que un error de tipos ahí hace fallar la compilación. `--release` es una opción de `typl` en su conjunto: funciona igual no solo con `-c`, sino también al ejecutar un archivo, en el REPL y con `typl test`. Un `compile-file` llamado dentro de un `typl` con `--release` también hace una compilación release, y `eprint`/`eprintln` se eliminan también del código evaluado con `eval` dentro del ejecutable así generado.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # igual si se escribe después de -c
```

`-O0`, `-O1`, `-O2` y `-O3` fijan el **nivel de optimización**. El valor por defecto es `-O0`, que no optimiza. A partir de `-O1`, el compilador hace primero optimizaciones sencillas, como sustituir una variable ligada solo a un literal por su valor o calcular de antemano la aritmética entera sobre constantes, y deja el resto a la optimización de LLVM del nivel del mismo nombre. En todos los niveles los resultados del cálculo son los mismos. Igual que `--release`, es una opción de `typl` en su conjunto: además del ejecutable que crea `-c`, afecta al código que se compila al ejecutar un archivo, en el REPL y en `typl test`, y a un `compile-file` llamado allí. El código que ejecuta el intérprete no se optimiza.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # igual si se escribe después de -c
```

Puedes hacer lo mismo llamando a `compile-file` desde el REPL o desde un programa.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Si compilas a menudo, puedes poner esta línea en un archivo y ejecutarlo con `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Los nombres de archivo se resuelven desde **el directorio actual donde se arrancó `typl`**, no desde la
ubicación de `build.typl`.

Para enlazar un `libtypelisp_front.a` colocado en un sitio distinto de donde busca `typl`, indica su carpeta
con `--lib-dir`. Se aplica tanto a `typl -c` como a `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Si la carpeta indicada no tiene `libtypelisp_front.a`, `typl` se detiene con un error. El archivo solo
funciona con el `typl` compilado junto con él. Tras recompilar `typl`, vuelve a copiarlo.

### 3.3 Qué puede contener un archivo compilado con AOT

- El nivel superior del archivo de entrada solo puede contener definiciones (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) y `use` `module`. No se permiten expresiones de nivel
  superior como `(println ...)`, salvo el `(main)` final. Pon el trabajo dentro de `main`.
- Sin un `main` que no reciba argumentos, la compilación falla con un error.
- Los archivos de los módulos usados también se compilan y se combinan en un único ejecutable.
- Las bibliotecas indicadas con `:library` en `defffi` se enlazan automáticamente ([FFI de C](ffi.md)).
- Todas las funciones de la biblioteca estándar se pueden usar con la compilación AOT. También se puede
  usar `eval`, pero entonces el comprobador de tipos y el intérprete entran en el ejecutable, que se vuelve
  más grande y más lento al arrancar. Los programas que no llaman a `eval` no los incluyen.

### 3.4 Cómo se comporta el ejecutable

- `(command-line-args)` devuelve un `Vector<string>` con la misma forma tanto si se ejecuta como
  `typl hello.typl a b` como si se ejecuta como `./hello a b`. El primer elemento es el nombre del programa.
- El código de salida se fija con `(exit n)`. Si `main` retorna normalmente, es 0.
- Ante un `panic`, el programa imprime el mensaje y sale con un código distinto de cero.

### 3.5 Recursos que usa el ejecutable

`typl -c` tiene opciones que indican qué recursos puede usar el ejecutable mientras se ejecuta.

```sh
$ typl --heap-cells 100000 -c hello.typl    # fija la arena de cons en 100000 celdas
$ typl -c hello.typl --max-memory 64M       # reserva como mucho 64 MiB
$ typl -c hello.typl --no-cons              # no usa celdas cons
```

- `--heap-cells N` fija la capacidad de la arena de celdas cons en `N` celdas. La arena no crece más allá: si la recolección de basura no puede liberar una celda, el programa se detiene con un error de ejecución. Sin la opción, la arena empieza con 65536 celdas y crece según haga falta.
- `--max-memory SIZE` limita la memoria total que reserva el ejecutable. Todo cuenta: la arena de cons, las cadenas, valores como vectores y tablas hash, y la memoria que usa el propio runtime. `SIZE` es un número de bytes y puede ir seguido de `K`, `M` o `G` (1024, 1024² y 1024³ veces). Una reserva que superaría el límite se informa y el programa termina con el código de salida 1. Esta salida no se puede capturar con `catch`. Sin la opción no hay límite.
- `--no-cons` crea un ejecutable que no usa celdas cons. No tiene arena de cons, así que un programa que haga algo que necesite celdas cons es un **error de compilación**, que muestra esa operación y la cadena de funciones desde `main` que llega a ella. Se necesitan celdas cons para construir una lista `Sexpr` (incluidas una lista citada y una construida con cuasicita) o un argumento `&rest`, y para `read`, `eval` y `macroexpand`. Las operaciones con números, cadenas, vectores, tablas hash, estructuras, canales y similares, y `print`/`println`/`format`, no usan ninguna. No se puede dar junto con `--heap-cells` y se escribe después de `-c`.

`--max-memory` se escribe después de `-c`. `--heap-cells` puede escribirse antes o después de `-c`, y entonces es la capacidad de la arena de cons del ejecutable. La arena de cons cuenta para el límite de memoria, así que con un límite en el que no cabe la arena el programa se detiene antes de empezar.

### 3.6 Compilar para WebAssembly

Con `--target wasm32-wasip1`, `typl -c` crea un módulo WebAssembly (un comando WASI preview 1) en lugar de un ejecutable. Sin `-o`, su nombre es el de la fuente con `.typl` cambiado por `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # crea hello.wasm
$ wasmtime hello.wasm a b                        # ejecutarlo con un entorno compatible con WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- Un `typl` instalado con `install.sh` se puede usar tal cual. Uno instalado con `cargo install` requiere preparación (que también sirve para la sección 3.7). Primero ejecute `rustup target add wasm32-wasip1 wasm32-unknown-unknown` con rustup. Luego instálelo con `--features wasm` (`cargo install --locked --path . --features wasm`).
  Un `typl` instalado sin ello no tiene el runtime de WebAssembly, y `--target` es un error. Para usar una compilación de depuración de `typl` en el repositorio, ejecute en su lugar `scripts/build-wasm-runtime.sh` en el repositorio.
- El enlazado usa el enlazador de la toolchain de rustup (`rust-lld`), así que rustup también es necesario al compilar.
- El módulo se comporta como el ejecutable: salida estándar, error estándar y código de salida, `(command-line-args)`,
  lectura y escritura de archivos, `throw`/`catch`/`panic`, tareas, canales y `sleep`, y `eval` funcionan.
  Las tareas se ejecutan una tras otra en un solo hilo. Solo se pueden leer y escribir archivos dentro de los
  directorios que el entorno muestra al módulo (en wasmtime, se indican con `--dir`).
- Un programa que usa algo que WebAssembly no tiene es un **error de compilación**. El error muestra lo usado y
  la cadena de funciones desde `main` que llega hasta ello. No se pueden usar: sockets (TCP, UDP, dominio Unix,
  TLS, resolución de nombres), hilos del sistema (`Thread`), funciones de C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  ni `ed`.
- WASI no tiene forma de conocer la zona horaria, así que la hora local es desconocida. `decode-universal-time` y `encode-universal-time` sin `zone`, y `get-decoded-time`, hacen panic en tiempo de ejecución. Funcionan si se pasa `zone`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` y `--release` funcionan igual que para un ejecutable.

### 3.7 Ejecutar en un navegador o en Node.js

Con `--target wasm32-unknown-unknown`, `typl -c` crea un módulo WebAssembly que se carga y ejecuta desde JavaScript. Junto al módulo (`hello.wasm`), en la misma carpeta, se crea el JavaScript que lo carga y ejecuta (`hello.mjs`). La preparación es la misma que en la sección 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # crea hello.wasm y hello.mjs
$ node hello.mjs                                       # ejecutarlo con Node.js
args: #()
fib(25) = 75025
```

Desde otro JavaScript, llame a `run` de `hello.mjs`. `write` recibe la salida como texto (`fd` es 1 para la salida estándar y 2 para el error estándar). El valor de `run` es el código de salida.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* mostrarlo en la página, por ejemplo */ } });
```

- Sin `write`, Node.js escribe en la salida estándar y el error estándar, y un navegador en la consola, línea a línea.
- En un navegador, sirva `hello.wasm` por HTTP desde el mismo lugar que `hello.mjs` (no se puede cargar desde `file://`).
- `sleep` espera con `Atomics.wait`. Un navegador solo lo permite dentro de un Worker de una página con aislamiento de origen cruzado, así que ejecute en un Worker los programas que usan `sleep`.
- La hora local (`decode-universal-time` sin `zone`, etc.) se calcula con la zona horaria que informa `Date` de JavaScript.
- Además de lo que no se puede usar en la sección 3.6, las operaciones con archivos y directorios, las variables de entorno, `(command-line-args)`, el directorio personal y `dribble` son un **error de compilación**. La entrada estándar se lee como vacía.

### 3.8 Compilar para un microcontrolador (Cortex-M)

Con `--target thumbv7em-none-eabihf`, `typl -c` crea un ejecutable (ELF) para una placa Cortex-M4F o M7 sin sistema operativo. Sin `-o`, recibe el nombre de la fuente con `.typl` cambiado por `.elf`. Con `--target thumbv8m.main-none-eabihf`, es para una placa Cortex-M33.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # crea hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # ejecútalo en el Cortex-M4 de QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # ejecutarlo en el Cortex-M33 de QEMU
```

- Un `typl` instalado con `install.sh` se puede usar tal cual. Para instalar con `cargo install`, ejecuta `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf` y luego instala con `--features embedded` (`--features wasm,embedded` para ambos). Con una compilación de depuración de `typl` en el repositorio, ejecuta `scripts/build-embedded-runtime.sh`. El enlazado usa el enlazador de la toolchain de rustup.
- Indica la memoria de la placa con `--linker-script FILE`. Se escribe como el `memory.x` de `cortex-m-rt`: las direcciones y tamaños de FLASH y RAM. Una línea como `_typelisp_stack_size = 32K;` fija cuánta RAM ocupa la pila (64KiB si no se indica); el resto de la RAM es el heap. Sin la opción, se usa la distribución del `mps2-an386` (`mps2-an505` para un Cortex-M33) de QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` elige quién proporciona la salida, la entrada, la terminación y el reloj (las funciones de la placa).
  - `semihosting` (predeterminado): mediante semihosting, para ejecutar en QEMU (`-semihosting-config enable=on`) o con un depurador.
  - `c`: la placa las proporciona como funciones de C. Pasa con `--link FILE` un objeto o biblioteca estática que defina las funciones siguientes (`--link` se puede repetir; una biblioteca del fabricante que llama `defffi` se pasa igual).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 es la salida estándar, 2 la salida de error
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; bytes leídos, 0 al final
  void     typelisp_host_exit(int32_t code);                              // no regresa
  uint64_t typelisp_host_now_us(void);                                    // microsegundos de un reloj que nunca retrocede
  int64_t  typelisp_host_unix_us(void);                                   // microsegundos desde 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // espera us microsegundos
  ```

- Los registros de una placa (periféricos asignados a memoria) se leen con `volatile-read-u8`, `volatile-read-u16` y `volatile-read-u32`, y se escriben con `volatile-write-u8`, `volatile-write-u16` y `volatile-write-u32`. La dirección es un `int` y el valor leído tiene el tipo de su anchura (`u8`, etc.). Cada uno lee o escribe exactamente una vez, donde está escrito; el compilador no lo omite ni lo mueve. Todos solo pueden usarse dentro de `unsafe`. Una dirección negativa, o que no sea múltiplo de la anchura, es un error en tiempo de ejecución antes de leer o escribir nada.

  ```lisp
  ;; UART 0 del mps2-an386 de QEMU: datos 0x40004000, estado 0x40004004, control 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; esperar mientras envía
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; habilitar el envío
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" y un salto de línea
  ```

- La función que se llama cuando llega una interrupción se escribe como un `defun` con el atributo `#[interrupt(N)]` (la interrupción de periférico `N`) o uno como `#[exception(SysTick)]`. Una interrupción se habilita con `(enable-interrupt N)`. Un manejador se ejecuta en el acto; solo si llega en medio del trabajo del entorno de ejecución se ejecuta cuando ese trabajo termina. Un manejador no puede esperar (`sleep` es un error de compilación). Los detalles están en la sección 3.17 de la [referencia de sintaxis](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; provocar la interrupción 5 uno mismo (el registro de pendientes del NVIC)
  ```

- El runtime inicia el programa (la tabla de vectores y la preparación de la RAM) y reserva la memoria. Un HardFault o una interrupción sin manejar termina el programa con el código de salida de un panic (1).
- `throw`/`catch`/`panic`, los bignums, las fracciones y los números de coma flotante, y las tareas, los canales y `sleep` se comportan como en el ejecutable. Las tareas se ejecutan por turnos en el único núcleo. Se pueden usar funciones de C (`defffi`).
- Un programa que usa algo que la placa no tiene es un **error de compilación**: además de lo que no se puede usar en la sección 3.7, `eval` y `macroexpand`. La hora local es desconocida (como en la sección 3.6).
- La arena de cons empieza con 1024 celdas y crece cuando hace falta. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` y `--release` funcionan igual.

### 3.9 Compilar para Arduino Uno (AVR)

Con `--target avr-atmega328p` se crea un ejecutable (ELF) para el microcontrolador de Arduino Uno (ATmega328P). El siguiente programa hace parpadear el LED de la placa.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; configurar como salida
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; invertir
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # crea blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # ejecutar en QEMU
```

- No hay runtime, así que un programa que lo use es un **error de compilación**. El error indica qué se necesita y a través de qué funciones se llega a ello. Se pueden usar: cálculos con enteros de ancho fijo (de `i8` a `u32`), `bool` y `char`, llamadas a funciones, condicionales, bucles, `defvar` y `defconstant` con esos valores, y `volatile-read-u8` y similares con la dirección escrita como número o constante.
- No se pueden usar: valores que viven en el heap, como cadenas, listas y estructuras; cálculos con `int`; la recursión, `catch` y las llamadas a valores de función; las llamadas a funciones de C; los manejadores de interrupciones; la división por una variable (la división por un número sí se puede usar); los accesos `volatile` cuya dirección se calcula.
- Un bucle que no hace nada desaparece con `-O1` o superior. En un bucle de espera, lea por ejemplo un registro.
- Cuando `main` termina, el microcontrolador se detiene.
- La disposición de la memoria se cambia con `--linker-script FILE`. Si se omite, son 32 KiB de flash menos los últimos 512 bytes del cargador de arranque, y 2 KiB de RAM. Las direcciones de RAM se escriben sumando `0x800000`, como `0x800100`.
- Los cálculos con `f32` y `f64` son un error de enlace, porque no hay funciones auxiliares para AVR.
- El enlace usa el enlazador de la toolchain de rustup, como en la sección 3.8.

## 4. Volcados

Puedes guardar las definiciones de la sesión actual en un archivo y arrancar desde él la próxima vez.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

También sirve para ejecutar un archivo, como en `typl --image session.typld prog.typl`.

- Lo que se guarda son las **definiciones**. Las expresiones evaluadas en la sesión no se guardan.
- Las funciones que compilaste con `compile` se guardan en su forma compilada.
- Las variables globales se restauran **volviendo a ejecutar sus inicializadores**, no con los valores que
  tenían cuando se escribió el volcado.
- Un volcado no lo puede cargar un `typl` de una versión distinta del que lo escribió (es un error).

Si ejecutas un archivo y haces `(dump ...)` desde él, las definiciones de ese archivo quedan en un módulo
con el nombre del archivo. Una función definida en `dp.typl` se llama `dp::sq`, y llamarla desde otro
archivo requiere `pub` ([Módulos y organización de archivos](modules.md)).

## 5. Sobre los archivos de módulos compilados

No existe un formato, como el `.fasl` de Common Lisp, para escribir en un archivo el resultado compilado de
cada módulo. `compile-file` construye el ejecutable directamente a partir de las fuentes. No quedan archivos
intermedios.

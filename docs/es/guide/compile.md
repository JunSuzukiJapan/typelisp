<!-- translated-from: docs/ja/guide/compile.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
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
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Sin `-o`, el ejecutable toma el nombre del archivo fuente sin `.typl` y se coloca en la misma carpeta que
el archivo fuente. Si el nombre del archivo fuente no termina en `.typl`, `-o` es obligatorio. Con `-c`
(`--compile`) no se pueden dar `--image`, `--heap-cells` ni `--feature`.

Puedes hacer lo mismo llamando a `compile-file` desde el REPL o desde un programa.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
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

- El nivel superior del archivo de entrada solo puede contener definiciones (`defun` `defmethod` `defvar`
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

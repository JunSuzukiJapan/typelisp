<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Para programadores de Common Lisp

typelisp hereda la sintaxis de Common Lisp (CL) y muchos de sus nombres de funciones, pero es un lenguaje
con tipado estático. Por eso, el código de CL no siempre funciona tal como está escrito. Esta guía reúne los
puntos donde suele tropezar quien está acostumbrado a CL, junto con cómo reescribir el código.

## 1. No hay `nil` ni `t`

Los valores booleanos son `true` y `false`. `nil` y `t` no están definidos.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Solo un `bool` puede ser condición.** Escribir `0` o una lista vacía como condición es un error de
  tipos. No existe la regla de que "todo lo que no sea nil es verdadero".
- **La rama else de `if` no se puede omitir.** `(if c x)` es un error. Cuando no hace falta rama else, usa
  `when` / `unless`.
- **"Sin valor" se expresa con `Option<T>`.** Una función que en CL devolvía nil para indicar "no
  encontrado" devuelve aquí `(some x)` o `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- La lista vacía `()` es, según el contexto, el valor del tipo Unit (el valor de retorno de una función
  que no devuelve nada) o la lista vacía de los datos de expresiones S. Es un valor distinto de `false`.

## 2. Escribir tipos

Los argumentos y los valores de retorno de las funciones deben tener tipos.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; una función genérica
  (unwrap-or (first (iter v)) default))
```

- No se puede escribir una definición sin tipos, como `(defun f (x) x)`.
- Las variables globales como `defvar` también necesitan tipo: `(defvar (count int) 0)`.
- `the` no es una comprobación en tiempo de ejecución sino una anotación para el comprobador de tipos.
- **No hay forma de examinar tipos en tiempo de ejecución.** No existen `typep` ni `type-of`, porque el tipo
  de cada valor queda fijado en tiempo de compilación. Para aceptar uno de varios tipos, crea un tipo suma
  con `defenum` o usa un trait.
- `deftype` define un alias de tipo. No se puede crear un tipo que describa un rango de valores, como
  `(deftype small () '(integer 0 9))`.

El tipo entero por defecto `int` tiene precisión arbitraria; como el integer de CL, su tamaño no tiene
límite superior. También existen los tipos de ancho fijo `i8` a `i32` y `u8` a `u32`. No hay tipo entero de
ancho fijo de 64 bits.

## 3. Funciones como valores

typelisp no separa los espacios de nombres de funciones y variables. El nombre de una función se puede pasar
como valor tal cual. No hay `#'` ni `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; se llama directamente, no con funcall

(apply-to twice 5)                        ; twice, no #'twice
```

- Las funciones incorporadas como `+` y `1+` también se pueden pasar como valores tal cual, donde el tipo
  del argumento está fijado, como en `(fn (int) int)`. Al pasar una a una función genérica como `foldl` o
  `map`, no se sabe a qué `+` de qué tipo se refiere, así que envuélvela en una `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Las funciones de secuencia reciben **primero la colección y después la función**: `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. Es al revés que el `(mapcar f list)` de CL.
- `lambda` no puede usar `&optional` ni `&key` (`&rest` sí).
- **Una función no se puede llamar antes de definirla.** En CL puedes llamar a una función que defines más
  tarde, pero aquí eso da `no such function`. Para funciones mutuamente recursivas, declara primero una de
  ellas con `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listas y Vector

Lo que corresponde a una lista de CL son los **datos de expresiones S**, cuyo tipo es `Option<Sexpr>`
(la lista vacía es `none`). `(list 1 2 3)` y `'(a b c)` tienen este tipo. Los datos de expresiones S son
algo con lo que trabajan las macros y `read`; para un contenedor de datos normal, usa **`Vector<T>`**.

| Lo que quieres | CL | typelisp |
|---|---|---|
| Cabeza y resto de una expresión S | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Recorrer una lista de expresiones S | `(dolist (x xs) ...)` | Igual |
| Una secuencia de elementos de un mismo tipo | Una lista o un vector | `Vector<T>` |
| Un par | `(cons a b)` | `(cons a b)` (su tipo es `cons-cell<A,B>`) |
| Aplicación (mapping) | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` son los accesores del par `cons-cell<A,B>` creado con `cons`. No se pueden usar sobre listas
de expresiones S.

Crear un `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Las funciones de secuencia como `map`, `filter`, `sort` y `find` trabajan sobre valores que implementan el
trait `Iter`. Pasa un `Vector` tras convertirlo en iterador con `(iter v)`.

## 5. No hay valores múltiples

No existen `values` ni `multiple-value-bind`. Las funciones que en CL devuelven varios valores devuelven
aquí un par o una estructura.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → un `cons-cell` cuyo `car` es 3 y cuyo `cdr` es 1 |
| `(decode-universal-time t)` → 9 valores | Una estructura `decoded-time` |
| `(read-from-string s)` → valor, posición | `(read-from-string s)` devuelve un `cons-cell` de valor y posición dentro de un `Result`. Para solo el valor, `(read s)` |

## 6. No hay variables especiales (enlace dinámico)

`let` siempre enlaza léxicamente. Si vuelves a enlazar con `let` una variable definida con `defvar`, las
funciones llamadas desde ahí siguen viendo el valor original.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 en CL, 1 en typelisp
```

Para cambiar temporalmente una variable de control como `*print-base*`, usa `dlet`. Asigna el valor y
restaura el original se salga como se salga del cuerpo.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` reescribe la propia variable global, así que no es un enlace por hilo.

## 7. No se adopta el sistema de condiciones

No existen `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` ni `signal`.
Encajan mal con el tipado estático. En su lugar se usan estas dos cosas con fines distintos:

- **Los fallos recuperables devuelven `Result<T,E>`.** Quien llama separa `ok` / `err` con `match`. No hay
  una forma abreviada como el `?` de Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Los fallos irrecuperables (errores de programación) son `panic`.** `(panic "message")`, pasar `none` a
  `unwrap`, dividir por 0 y un índice fuera de rango son de esta clase, y el programa se detiene. La
  limpieza de `unwind-protect` se ejecuta antes de detenerse.

Los tipos de error se unifican con el trait `Error`, y `(message e)` da el mensaje. Cómo crear tu propio
tipo de error está en
[Option, Result y tipos de error](../reference/functions/option-result.md#3-tipos-de-error-y-el-trait-error).
`assert` y `warn` se pueden usar como en CL.

Existen `catch` / `throw` / `unwind-protect`. Sin embargo, la etiqueta de `catch` se limita a un símbolo
literal sin evaluar (`'done`), y los valores lanzados con una etiqueta tienen un único tipo.

## 8. No hay CLOS

No existen `defclass`, `defgeneric` ni la combinación de métodos.

- Los tipos de datos se definen con `defstruct` (estructuras) y `defenum` (tipos suma).
- `defmethod` define métodos cuyo destino se decide solo por **el tipo estático del primer argumento**. No
  hay despacho múltiple.
- Para dar operaciones comunes a varios tipos, usa traits (`deftrait` / `impl`). Para valores cuyo tipo
  concreto se decide en tiempo de ejecución, usa el tipo `:dyn Trait`
  ([Referencia de sintaxis 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

En qué se diferencia `defstruct`:

- El constructor es `NombreDeTipo::new`: `(point::new 1 2)`. Si quieres un nombre como `make-point`, la
  opción `(:constructor make-point)` lo crea.
- Además de `(x p)`, un accesor se puede escribir `p::x`. Se cambia con `(setf p::x 5)`.
- No se crea ningún predicado (`point-p`). No hay `:conc-name`, `:type` ni `:named`.
- `:include` solo hereda las ranuras; el tipo no se convierte en subtipo del padre.

## 9. Módulos en lugar de paquetes

No hay paquetes. Los espacios de nombres son módulos, y un archivo es un módulo por sí mismo. En lugar de
`pkg:symbol`, se escribe `module::name`, y los nombres se traen con `use`
([Módulos y organización de archivos](modules.md)).

Existen las palabras clave `:foo`, que son símbolos que se evalúan a sí mismos. Como no hay paquetes, los
dos puntos forman parte del nombre: `(symbol->string :foo)` devuelve `":foo"`.

## 10. Diferencias de lectura y sintaxis

- No se distinguen mayúsculas y minúsculas (los símbolos pasan a minúsculas al leerse). Igual que en CL.
- No existe `#'` (sección 3). Los literales de números complejos `#c(...)` no se pueden leer; los números
  complejos se crean con `(complex 1.0 2.0)`.
- Las cláusulas del `loop` extendido se escriben con palabras clave: `(loop :for i :from 1 :to 3 :collect i)`.
  Un `loop` que no empieza por una palabra clave es un simple bucle infinito, del que se sale con `(break)`
  o `(return valor)`. `return` sale del bucle más interno (para salir de una función, usa `return-from`).
- El destino de `format` es `false` (devolver una cadena), `true` (salida estándar) o un stream. Las
  directivas de formato son las mismas que en CL.
- Leer de una cadena es `(read "...")`, y leer de un stream es `(read-sexpr s)`. Ambas devuelven un
  `Result`.
- `eval` comprueba los tipos de la expresión dada antes de evaluarla y devuelve un `Result`. Las referencias
  anticipadas no son posibles, igual que en el código fuente.
- No existe `eval-when`.
- Los nombres de función no usan los sufijos `?` ni `!`. Los predicados se nombran con `-p` / `p` como en CL
  (`zerop`, `sexpr-null`) o con `is-` delante (`is-some`).

## 11. Funciones principales con nombres distintos

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (de un stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versiones de dos argumentos de `floor` y compañía | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (orden de argumentos invertido; sección 4) |
| `length` (de un vector) | `len` |
| `hash-table-count` | `count` / `size` |

La lista de funciones está en [Funciones incorporadas](../reference/functions/README.md).

## 12. Otras cosas que no existen

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` y `copy-readtable`, `readtable-case` (las macros de lectura en sí se pueden definir con
  `set-macro-character`)
- Nombres de ruta lógicos y nombres de ruta con comodines
- `input-stream-p` / `output-stream-p` (la dirección de un stream la decide su tipo)

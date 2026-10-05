<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Fundamentos de tipos

typelisp es un lenguaje con tipado estático. Este capítulo explica qué hace por ti el comprobador de
tipos, los tipos que más usarás (`Option`, `Result`, estructuras y enumeraciones) y los genéricos. Se
supone que has leído [Primeros pasos](intro.md).

## 1. Qué significa el tipado estático

En typelisp, el tipo de cada expresión queda fijado antes de ejecutar el programa. Una expresión cuyos
tipos no encajan es un error antes de que se ejecute nada.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; error de tipos

(main)
```

Al ejecutar este archivo se detiene con un error de tipos sin llegar siquiera a imprimir `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Hay que escribir tipos para los argumentos y valores de retorno de las funciones, las variables globales
y los campos de las estructuras. El tipo de una variable de `let` se toma de su valor inicial.

Los tipos principales:

| Tipo | Valores de ejemplo |
|---|---|
| `int` | `42`, `-7` (enteros de precisión arbitraria) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Enteros de ancho fijo |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | El tipo de retorno de una función que no devuelve valor |

No hay forma de preguntar por el tipo de un valor en tiempo de ejecución (no existen `typep` ni `type-of`
de Common Lisp), porque todos los tipos se conocen ya antes de ejecutar el programa.

## 2. `Option<T>`: un valor que puede faltar

typelisp no tiene `nil`. "Puede que no haya valor" se expresa con el tipo `Option<T>`. Un valor de
`Option<T>` es o bien `some`, que contiene un valor de `T`, o bien `none`, que no contiene nada.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` no es `int`, así que no se puede usar en aritmética tal cual. `(+ (safe-div 10 2) 1)` es un
error de tipos. Para usar lo que hay dentro, separa `some` de `none` con `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- En la rama `(some q)`, el contenido queda ligado a la variable `q`.
- `match` comprueba que sus ramas **cubren todos los casos**. Olvidar la rama `(none)` es un error de tipos.

### Por qué no hay nil

En muchos lenguajes, `nil` (`null`) puede ocupar el lugar de un valor de cualquier tipo. Como resultado,
olvidar tratar el caso "sin valor" pasa desapercibido hasta que se ejecuta el programa. En typelisp, un
lugar donde puede faltar un valor tiene el tipo `Option<T>`, y el código no supera el comprobador de tipos
a menos que `match` trate el caso `none`. Un caso olvidado se descubre antes de ejecutar el programa.

Las condiciones siguen la misma idea: solo un `bool` puede ser la condición de `if`. No existe una regla
como la de Common Lisp de "todo lo que no sea `nil` es verdadero".

### Operaciones habituales

| Forma | Significado |
|---|---|
| `(unwrap-or opt default)` | El contenido si es `some`; el valor por defecto si es `none` |
| `(unwrap opt)` | Saca el contenido. Detiene el programa si es `none` |
| `(is-some opt)` / `(is-none opt)` | Comprueba cuál de los dos es |

Muchas funciones de la biblioteca estándar devuelven `Option`. Por ejemplo, `position` devuelve la posición
dentro de `some` si encuentra el elemento, y `none` si no.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: una operación que puede fallar

Una operación que puede fallar devuelve `Result<T,E>`: `ok`, que contiene un valor de `T` si tiene éxito,
o `err`, que contiene un error `E` si falla.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Tus propias funciones también pueden devolver `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Usa `Option` cuando la falta de un valor no necesita explicación, y `Result` cuando quieras decir por qué
algo falló. [Manejo de errores](errors.md) trata en detalle el manejo de errores.

## 4. `defstruct`: estructuras

Un tipo con campos con nombre se define con `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

La definición te da lo siguiente:

```lisp
(let ((p (point::new 3 4)))     ; crear uno (argumentos en el orden de los campos)
  (println "~a" p::x)           ; leer un campo; (x p) también vale
  (setf p::x 10)                ; cambiarlo
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Para dar a una estructura funciones propias, usa `defmethod`. El tipo del primer argumento (`self`)
decide a qué tipo pertenece el método.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Escribir solo el nombre del tipo en lugar de un argumento `self` crea una función que se llama como
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: una de varias formas

Un valor que es una de varias formas, como "un círculo, un rectángulo o un punto", se define con
`defenum`. Cada forma se llama **variante**. Cada variante puede contener un número y tipo de valores
distintos.

```lisp
(defenum shape
  (circle int)        ; radio
  (rect int int)      ; ancho y alto
  (dot))              ; no contiene ningún valor
```

Los valores se crean con el nombre del tipo delante, como en `shape::circle`. En `match`, se desarman por
el nombre de la variante.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Aquí también `match` comprueba que se cubren todos los casos. Si más adelante añades una variante a
`shape`, cada `match` que no la trate se convierte en un error de tipos, así que no se escapa ningún
lugar que haya que corregir.

Tras `(use shape)` puedes escribir `(rect 5 6)` sin el nombre del tipo.

`Option` y `Result` son enumeraciones construidas con este mismo mecanismo.

## 6. Genéricos

Una función que sirve para cualquier tipo se define con un **parámetro de tipo** `<T>` tras su nombre.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Al llamarla no se da el tipo. `T` se deduce de los argumentos.

```lisp
(first-or ints 7)          ; T es int
(first-or names "none")    ; T es string
(first-or ints "none")     ; error de tipos: ints es un Vector<int>, así que T es int
```

Las estructuras y las enumeraciones también pueden ser genéricas. `Vector<T>`, `Option<T>` y
`Result<T,E>` son tipos de esta clase.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Dentro de una función genérica no se sabe nada de `T`, así que no se pueden comparar ni sumar valores de
`T`. Para exigir algo como "cualquier tipo que se pueda comparar", usa traits ([Traits](traits.md)).

## 7. Dar otro nombre a un tipo

`deftype` da otro nombre a un tipo.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` es solo otra forma de escribir `int`, no un tipo nuevo. Pasar un `int` normal donde se espera
`meters` no es un error. Si quieres mantenerlos separados, crea una estructura, como en
`(defstruct meters (value int))`.

## 8. Qué leer después

- [Traits](traits.md): dar a los tipos operaciones comunes
- [Tipos](../reference/types.md): los tipos incorporados y los traits que implementa cada uno

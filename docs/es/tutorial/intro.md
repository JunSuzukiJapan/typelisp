<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Primeros pasos

Empezando por evaluar expresiones en el REPL, este capítulo recorre, por orden, funciones, variables,
condicionales, bucles, y listas y `Vector`. Para compilar `typl`, consulta el
[README.md](../../../README.md).

## 1. Iniciar el REPL

Si se inicia sin argumentos, `typl` entra en el REPL (modo interactivo). Escribe una expresión tras
`typl>` y se evalúa en el acto y se imprime su valor. `:quit` sale del REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

A partir de aquí, la entrada y los resultados del REPL se muestran de esta forma.

## 2. Evaluar expresiones

typelisp es un Lisp, así que una expresión va entre paréntesis con **el operador o el nombre de la
función al principio**. Se escribe `(+ 1 2)`, no `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Los números son de estas clases:

- Los **enteros** tienen el tipo `int`. Su tamaño no tiene límite superior.
- Los **decimales** tienen el tipo `f64`. Se escriben con punto decimal, como `1.5` o `2.0`.
- No se pueden mezclar `int` y `f64` en un cálculo. `(+ 1 2.0)` es un error de tipos. Para convertir,
  escribe `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` entre dos enteros da un entero descartando la parte fraccionaria (no produce una fracción como en
Common Lisp). Para el resto, usa `(mod 7 2)`.

Los valores booleanos son `true` y `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Definir funciones

Las funciones se definen con `defun`. **Los tipos de los argumentos y el tipo de retorno se escriben
siempre.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` significa "un argumento `n` de tipo `int`". Con varios argumentos, se enumeran:
  `((a int) (b int))`.
- El `int` que sigue a la lista de argumentos es el tipo de retorno.
- El valor de la última expresión del cuerpo es el valor de retorno de la función. No se escribe
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Una llamada cuyos tipos no encajan se informa como error de tipos **antes de ejecutarse**. Al ejecutar un
archivo, basta un solo error de tipos en cualquier parte para que no se ejecute ni una línea del programa.

Para que un argumento sea opcional, usa `&optional`. Si das un valor por defecto, el argumento toma ese
valor cuando se omite.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

El `false` que se pasa como primer argumento de `format` significa "devolver el resultado como cadena
en lugar de imprimirlo". Cada `~a` se sustituye por el siguiente argumento.

## 4. Variables

Las variables locales se crean con `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- El tipo de una variable de `let` se toma de su valor inicial. No hace falta escribirlo.
- Las variables de un mismo `let` no pueden referirse unas a otras. Para construir una variable a
  partir de la anterior, usa `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Para cambiar el valor de una variable, usa `setf`. **La asignación no puede cambiar el tipo de la
variable.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Las variables globales se definen con `defvar`. Aquí sí se escribe el tipo.

```lisp
(defvar (counter int) 0)
```

## 5. Condicionales

### if

Se escribe `(if condición expresión-si-verdadero expresión-si-falso)`. **La expresión del caso falso no
se puede omitir.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Solo una expresión de tipo `bool` puede ser condición. Escribir un número, como en `(if 0 ...)`, es un
  error de tipos.
- Las expresiones de los casos verdadero y falso deben tener el mismo tipo.

Cuando no debe ocurrir nada en el caso falso, usa `when` (y `unless` para lo contrario).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Con tres o más condiciones, `cond` se lee mejor. El `else` final se toma cuando no se cumple ninguna de
las condiciones.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Para ramificar según la forma de un valor, usa `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` encaja con cualquier valor. Como `int` tiene incontables valores, omitir la rama `_` es un error que
dice que no se cubren todos los casos. Donde `match` luce de verdad es al desarmar `Option` y los tipos
que defines tú mismo, que aparecen en el siguiente capítulo, [Fundamentos de tipos](types.md).

## 6. Bucles

Una función puede llamarse a sí misma.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Para un número fijo de repeticiones, usa `dotimes`. `i` va de 0 a `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

También existen `while`, `do` y el `loop` extendido de Common Lisp. Las palabras de cláusula del `loop`
extendido se escriben como palabras clave (`:for`, `:collect`, etc.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Listas y Vector

### Vector

Para guardar una secuencia de valores del mismo tipo, usa `Vector<T>`. `T` es el tipo de los elementos.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; imprime #<vector<int> 3 1 2>
```

- `(Vector::new)` por sí solo no determina el tipo de los elementos, así que se da el tipo con
  `(the Vector<int> ...)`.
- `(push v x)` añade al final, `(get v i)` lee el elemento `i` y `(len v)` da la longitud.
- Un `get` con un índice fuera de rango detiene el programa con un error.

### lambda y funciones de orden superior

Las funciones anónimas se crean con `lambda`. Igual que con `defun`, se escriben los tipos de los
argumentos y del retorno.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` y compañía reciben un `Vector` convertido en **iterador** con
`(iter v)`. Primero va la colección y después la función. El `v` de arriba se ligó con `let`, así
que no se puede usar fuera de ese `let`. El siguiente ejemplo define primero `v` con `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Para procesar los elementos uno a uno, usa `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Una función que recibe una función como argumento escribe el tipo de ese argumento como
`(fn (tipos-de-argumentos...) tipo-de-retorno)`. Una función definida con `defun` se puede pasar como
valor por su nombre.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listas (expresiones S)

Las listas creadas con `'(1 2 3)` o `(list 1 2 3)` son **datos de expresiones S**. Sus elementos no
tienen por qué compartir tipo.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Los datos de expresiones S sirven sobre todo para manejar los propios programas, en macros
([Macros](macros.md)) y con `read`. Para datos cuyos elementos tienen un tipo conocido, usa `Vector<T>`.
Una lista de expresiones S se puede recorrer con `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Un par de dos valores se crea con `cons` y se desarma con `car` y `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Escribir un programa en un archivo

Un programa se puede escribir en un archivo (con la extensión `.typl`) y ejecutar con
`typl nombre-de-archivo`. Usa `println` para mostrar resultados.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` imprime con las mismas directivas que `format` y termina con un salto de línea. `print` no
  añade el salto.
- `~a` incrusta un valor en forma legible para personas, y `~s` en una forma que se puede volver a leer
  (las cadenas llevan sus `"`).
- Un archivo se lee de arriba abajo. **Una función no se puede llamar antes de su definición.**

## 9. Qué leer después

- [Fundamentos de tipos](types.md): `Option`, `Result`, estructuras, enumeraciones, genéricos
- [Para programadores de Common Lisp](../guide/from-common-lisp.md): una lista de diferencias para
  quien conoce Common Lisp

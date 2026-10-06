<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Macros

Una macro es una función que recibe un programa y devuelve un programa. Las macros permiten crear nueva
sintaxis que las funciones no pueden expresar. Las macros de typelisp funcionan igual que `defmacro` de
Common Lisp. Este capítulo supone que has leído "Listas (expresiones S)" en [Primeros pasos](intro.md).

## 1. En qué se diferencian las macros de las funciones

Una función recibe sus argumentos **después de evaluarlos**. Una macro los recibe **como expresiones,
antes de evaluarlos** (como datos de expresiones S), construye otra expresión y la devuelve. La expresión
devuelta sustituye a la llamada a la macro, y solo entonces se comprueba su tipo y se ejecuta. Esta
sustitución se llama **expansión**.

Por ejemplo, una sintaxis como `unless` no se puede escribir como función. Como función, el cuerpo se
evaluaría primero incluso cuando la condición es verdadera.

## 2. `defmacro` y la cuasicita

Hagamos `my-unless`, que ejecuta su cuerpo solo cuando la condición es falsa.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Los argumentos de una macro no llevan tipos escritos. Todos los argumentos son datos de expresiones S.
- `&rest body` recibe los argumentos restantes juntos como una lista.
- Una expresión que empieza por `` ` `` (cuasicita) se construye como datos, tal como está escrita. Dentro
  de ella:
  - `,test` inserta el contenido de la variable `test` en esa posición.
  - `,@body` empalma los elementos de la lista `body` en esa posición.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Puedes comprobar la expansión con `macroexpand-1`. Al escribir una macro, mirar primero su expansión es la
forma más rápida de avanzar.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Las expansiones también pasan el comprobador de tipos

La expresión que devuelve una macro se comprueba como cualquier expresión escrita a mano.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

El error se informa en el lugar donde se llamó a la macro.

Las reglas de que la rama else de `if` no se puede omitir y de que ambas ramas de un `if` deben tener el
mismo tipo se aplican tal cual a las expansiones. El `my-unless` de arriba termina con
`(progn ,@body ())` para que, sea cual sea el tipo de la última expresión del cuerpo, ambas ramas del
`if` tengan tipo `()`.

## 4. Choques de nombres y `gensym`

Una macro directa que intercambia los valores de dos variables tiene este aspecto:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Funciona casi siempre, pero falla cuando la variable de quien llama se llama casualmente `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (no se intercambiaron)
```

La expansión es `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, y el `tmp` que creó la macro oculta
el `tmp` de quien llama.

Para evitarlo, crea los nombres de las variables que se usan dentro de una macro con `gensym`. `gensym`
devuelve un símbolo nuevo que no se puede escribir en ningún lugar de un programa.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Como en Common Lisp, las macros de typelisp no evitan automáticamente los choques de nombres (no son
higiénicas). Recuerda: **usa `gensym` para los enlaces que crea una macro.**

Del mismo modo, una macro que repite su cuerpo un número dado de veces se puede escribir así:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Expandir de forma distinta según los argumentos

El cuerpo de una macro es código typelisp normal, así que puede examinar sus argumentos con `if` o `match`
y construir una expansión distinta. Los argumentos son datos de expresiones S (`Option<Sexpr>`), y la
lista vacía es `none`.

Hagamos `my-and`, que devuelve `true` si todas sus condiciones son verdaderas.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; sin argumentos
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; solo uno
         `(if ,f (my-and ,@more) false)))             ; dos o más
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` es un patrón que toma la cabeza de una lista en `f` y el resto en `more`.
- `sexpr-null` comprueba si unos datos de expresiones S son la lista vacía.
- La rama final `_` es necesaria porque los datos de expresiones S tienen formas además de las listas
  (números, cadenas, etc.), y `match` exige cubrirlas también. Un argumento `&rest` es siempre una lista,
  así que esta rama nunca se ejecuta en realidad.
- Una macro puede llamarse a sí misma en su expansión. La expansión se repite hasta que no quedan
  llamadas a macros.

## 6. Argumentos opcionales

`&optional` recibe argumentos que se pueden omitir. Se pueden dar valores por defecto.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` recibe argumentos de palabra clave
([Referencia de sintaxis 3.14](../reference/syntax.md#314-defmacro--definición-de-macros)).

## 7. `macrolet`: macros para un solo lugar

Una macro que solo se usa dentro de una expresión se puede definir con `macrolet`. No es visible fuera.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Cosas a tener en cuenta

- **Una macro solo se puede llamar después de su definición.** Como con las funciones, defínela cerca del
  principio del archivo.
- Haz que una macro esté disponible para otros módulos con `(pub defmacro ...)`.
- Gran parte de la sintaxis estándar, incluidas `when`, `unless`, `cond`, `and`, `or` y `dotimes`, está
  definida como macros. Puedes ver su contenido con `(macroexpand '(when true 1))`.
- Si algo se puede escribir como función, escríbelo como función. Las macros no se pueden pasar como
  valores, y hay que leer su expansión para entender lo que hacen.

## 9. Qué leer después

- [Manejo de errores](errors.md): `Result`, `panic`, `catch` / `throw`
- [Funciones relacionadas con macros](../reference/functions/system.md#8-macros): `gensym`, `macroexpand`
  y más

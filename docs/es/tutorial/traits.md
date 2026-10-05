<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits

Un trait es una promesa de que "este tipo admite estas operaciones". Los traits permiten que varios tipos
compartan operaciones con el mismo nombre, de modo que una función que las usa no tenga que escribirse una
vez por tipo. Funcionan casi exactamente como los traits de Rust. Este capítulo supone que has leído
[Fundamentos de tipos](types.md).

## 1. Definir e implementar un trait

Definamos como el trait `Shape` las operaciones que devuelven el área y el nombre de una figura.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- El `()` tras el nombre del trait es la lista de traits de los que hereda (sección 4). Se deja vacía si
  no hay ninguno.
- Cada línea declara un método. `Self` representa "el tipo que implementa este trait".

Para implementar un trait para un tipo, escribe un `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Los métodos implementados se llaman igual que las funciones normales.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Omitir aunque sea uno de los métodos que declara el trait es un error de tipos en el `impl`.

## 2. Restricciones de trait: "cualquier tipo que implemente este trait"

Se puede poner una condición al parámetro de tipo de una función genérica con `where`. Esto se llama
**restricción de trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Gracias a `(where (Shape T))`, el cuerpo puede usar `name` y `area` sobre valores de `T`. Sin la
restricción no se sabría nada de `T`, así que no se podrían llamar.

Pasar un tipo que no implementa `Shape` es un error de tipos en la llamada.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Una función genérica obtiene su propia copia para cada tipo con el que se llama. No intervienen
comprobaciones de tipo ni bifurcaciones en tiempo de ejecución.

## 3. Implementaciones por defecto

Si un método de un trait tiene cuerpo, ese cuerpo se usa cuando un `impl` omite el método.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe es la implementación por defecto

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; la que se escribe aquí tiene prioridad

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementar traits estándar

La biblioteca estándar también tiene traits. Implementar uno hace que las funciones estándar que lo usan
estén disponibles para tu tipo.

| Trait | Métodos a implementar | Qué permite |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, el patrón `(= expr)` de `match`, etc. |
| `Ord` | `less` | `less-equal`, `greater`, etc. `Ord` hereda de `Eq` |
| `print-object` | `print-object` | Cómo muestran los valores `println` y compañía |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort`, etc. |
| `Error` | `message`, `source` | Usarlo como tipo de error ([Manejo de errores](errors.md)) |

Implementemos `Eq` y `Ord` para un tipo que representa una cantidad de dinero. Como `Ord` hereda de
`Eq`, el `impl` de `Eq` tiene que ir primero.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (la implementación por defecto de Ord)
```

Implementar `print-object` decide cómo muestra `println` el valor. El argumento `escape` es `true` cuando
se pide una forma que se pueda volver a leer, como con `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Combinado con una restricción de trait, puedes escribir una función que sirva para cualquier tipo que
implemente `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Dado un `Vector` con valores `money` de 300, 900 y 100 en ese orden, devuelve `(some 900 yen)`.

## 5. `:dyn`: manejar juntos valores de tipos distintos

Todos los elementos de un `Vector<T>` tienen el mismo tipo, así que los valores `circle` y `rect` no
pueden ir en un mismo `Vector<circle>`. Para manejar juntos "cosas que implementan `Shape`", usa el tipo
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Un valor `circle` o `rect` colocado donde se espera un `:dyn Shape` se convierte automáticamente.
- Qué `area` ejecuta la llamada `(area s)` se decide en tiempo de ejecución según el tipo de lo que
  contiene `s`.
- Colocar un valor cuyo tipo no implementa `Shape` donde se espera un `:dyn Shape` es un error de tipos.

Cómo elegir entre las restricciones de trait de la sección 2 y `:dyn`:

| | Restricción de trait (`where`) | `:dyn Trait` |
|---|---|---|
| Cuándo se decide el método llamado | Antes de ejecutar | En tiempo de ejecución |
| Mezclar tipos en un mismo `Vector` | No es posible | Es posible |
| Tipos utilizables | Sin restricción | Estructuras, enumeraciones, `int`, `string`, `f64` y otros (no `bool`, `char`, `symbol`, `i32` y similares) |

La lista exacta de tipos que se pueden usar está en
[Referencia de sintaxis 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Algunos traits no se pueden usar con `:dyn`: aquellos cuyos métodos usan `Self` para un argumento que no
sea `self` o para el valor de retorno (como `equals` de `Eq`). Como el tipo no se conoce hasta el tiempo
de ejecución, no hay forma de producir "un valor del mismo tipo".

## 6. Restricciones

- Mantén la definición de un trait, sus `impl` y el código que lo usa mediante `:dyn` en un mismo módulo
  (archivo). Todavía no se puede hacer visible un trait a otros módulos.
- Los tipos y los traits comparten un espacio de nombres. Dentro de un módulo, un tipo y un trait no
  pueden tener el mismo nombre.

## 7. Qué leer después

- [Macros](macros.md): definir sintaxis propia
- [Referencia de sintaxis 3.9](../reference/syntax.md#39-deftrait--impl--traits): implementaciones
  generales (blanket), tipos asociados y más
- [Traits estándar](../reference/functions/traits.md): la lista de traits de la biblioteca estándar

<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Traits estándar

Los traits para la iteración, la comparación, la aritmética y la adición y extracción de elementos. Los demás traits estándar están en sus propios
capítulos: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipos de error](option-result.md#3-tipos-de-error-y-el-trait-error)), `print-object`
([Impresión](printing.md#5-print-object-representación-impresa-por-tipo)), y los traits de streams y
`Pathish` ([Streams y archivos](streams-files.md)). Qué tipos implementan cuáles está en
[Tipos](../types.md). Cómo definir traits está en la
[Referencia de sintaxis](../syntax.md#39-deftrait--impl--traits).

## 1. El trait `Iter` y la iteración

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementan `Iter` mediante `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectivamente (el iterador se obtiene con `(iter colección)`).
`Chan<T>` es él mismo un `Iter` (`recv` hace el papel de `next`; [Canales](concurrency.md#2-chant--canales)).
Las listas `Sexpr` no implementan `Iter` (sus tipos de elemento no son uniformes). Si implementas `Iter`
para tu propio tipo, se puede recorrer con `doiter` tal cual y pasar a las
[funciones de secuencia](sequences.md#4-funciones-de-secuencia-sobre-iter).

## 2. `Equal` / `Ord` (comparación)

`Equal` es el trait que usa `equal` y corresponde al `PartialEq` de Rust. `Ord` corresponde al
`PartialOrd` de Rust. Se usan en las cotas `:where` de las funciones genéricas para exigir que el tipo de
los elementos pueda compararse.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; obligatorio
(deftrait Ord (Equal)                                     ; hereda de Equal
  (defmethod less (self (other Self)) bool)                  ; obligatorio
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Para implementar `Equal` se escribe `equal`, y para `Ord` solo `less`. Las implementaciones por defecto
completan el resto de `Ord`. `Ord` hereda de `Equal`, así que hace falta `impl Equal X` antes de `impl Ord X`.

`(equal a b)` llama a la implementación de `Equal` del tipo de `a`. Escribirlo para un tipo que no
implementa `Equal` es un error de tipo. Para comparar por identidad se usa `eq`/`eql`, y por estructura,
`equalp`. Estos tres se pueden usar con un valor de cualquier tipo.

Cada método de trait puede llamarse directamente como función (dentro de una cota `where (Equal A)`/
`(Ord A)`, o sobre un tipo concreto que lo implemente):

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Si son iguales (el `==` de Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` está implementado para: todos los tipos numéricos (`i8` a `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` y las tuplas (elemento a elemento,
cuando los elementos son `Equal`), y `Option<T>` y `Result<T,E>` (cuando son la misma variante y su
contenido es `equal`). En los tipos integrados, el método integrado `equal` del propio tipo es tal cual la
implementación de `Equal`. Los números se comparan con `=`, y las cadenas por contenido, distinguiendo
mayúsculas y minúsculas. `Ord` está implementado para: todos los tipos numéricos, `char` `string` y
`cons-cell<A,B>` (lexicográficamente, cuando los elementos son `Ord`).

El `equal` de una lista (`Sexpr`) es el `equal` de CL: los conses se comparan por estructura, las cadenas
por contenido y cualquier otro átomo con `eql`. Esta comparación recorre los valores en tiempo de
ejecución y no llama a ningún método. Por eso un struct dentro de una lista se compara por identidad, como
en CL, sea cual sea el `impl Equal` de su tipo.

Los nombres de los métodos de `Ord` no se solapan con los operadores integrados (`< <= > >=`) ni con `lt`
porque lo integrado no puede redefinirse y cada implementación delega en ellos. Los operadores de
comparación escalares son métodos integrados por tipo de receptor ([Números](numbers.md),
[Cadenas y caracteres](collections.md)). Dentro de una cota, el operador escrito se lee como el método del
trait (capítulo 3). `=` pasa a ser `equal`, y `/=` pasa a ser `(not (equal a b))`.

## 3. Traits aritméticos (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Una capa para que el código genérico exija "un tipo que se pueda sumar". **La aritmética sobre tipos
concretos usa los operadores incorporados** ([Números](numbers.md)) y no pasa por esta capa.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; la distancia es siempre int (como en ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; sin métodos; una combinación de cinco
```

**Dentro de una restricción se pueden escribir operadores.** Cuando el receptor es una variable de tipo
ligada por `:where`, los operadores se leen como métodos de trait (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

El método del trait no se llama `+` porque `+` es el nombre de un método incorporado e `impl` se niega a
redefinirlo (`cannot redefine built-in method`). No hay `Neg`: `(- x)` se expande a `(- (- x x) x)`, así
que basta con `Sub`.

Implementados para: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` en todos los tipos numéricos (excepto
`complex`), y `Bits` en todos los tipos enteros y en `int`.


**`Div` nombra su tipo de resultado con el tipo asociado `Output`.** Como en CL, `/` sobre dos enteros
es un `rational`, así que el `Output` de `int` es `rational` y el de cualquier otro tipo numérico es el
propio tipo. Como `Output` difiere según el tipo, `Number` no incluye `Div` (una lista de supertraits
debe fijar cada tipo asociado). Una función genérica que divide recibe el tipo del resultado con
`(:where (Div T (Output R)))`, o pide una división cerrada en `T` con `(:where (Div T (Output T)))`
(entonces no se puede pasar un `int`).

## 4. El trait `Push` (añadir elementos)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Una colección a la que se pueden añadir elementos de uno en uno. Una función genérica pide «un tipo
al que se puede añadir `T`» con `(:where (Push C (Item T)))` y añade a cualquiera de esas
colecciones con el mismo `(push c x)`.

| Tipo | Qué hace `push` |
|---|---|
| `Vector<T>` | Añade al final |
| `Deque<T>` | Añade al final (`push-back`) |
| `Array<T>` | Añade en el puntero de relleno (`push-extend`). Panic sin puntero de relleno |
| `HashSet<T>` | Lo pone en el conjunto (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Un tipo propio que implemente `Push` se puede pasar a las mismas funciones genéricas. También puede
ser un objeto trait, como en `:dyn Push<int>`.

## 5. El trait `Pop` (sacar elementos)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Una colección de la que se puede volver a sacar el último elemento añadido con `push`. `none` si
está vacía. Una función genérica la pide con `(:where (Pop C (Item T)))`.

| Tipo | Qué hace `pop` |
|---|---|
| `Vector<T>` | Saca el último elemento |
| `Deque<T>` | Saca el último elemento (`pop-back`) |
| `Array<T>` | Saca el elemento justo antes del puntero de relleno. Panic sin puntero de relleno |

Un `HashSet<T>` no tiene «último elemento», así que es `Push` pero no `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Como con `Push`, un tipo propio puede implementarlo, y puede ser un objeto trait, como en
`:dyn Pop<int>`.

<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits estándar

Los traits para la iteración, la comparación y la aritmética. Los demás traits estándar están en sus propios
capítulos: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipos de error](option-result.md#3-tipos-de-error-y-el-trait-error)), `print-object`
([Impresión](printing.md#5-print-object-representación-impresa-por-tipo)), y los traits de streams y
`Pathish` ([Streams y archivos](streams-files.md)). Qué tipos implementan cuáles está en
[Tipos](../types.md). Cómo definir traits está en la
[Referencia de sintaxis](../syntax.md#39-deftrait--impl--traits).

## 1. El trait `Iter` y la iteración

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementan `Iter` mediante `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectivamente (el iterador se obtiene con `(iter colección)`).
`Chan<T>` es él mismo un `Iter` (`recv` hace el papel de `next`; [Canales](concurrency.md#2-chant--canales)).
Las listas `Sexpr` no implementan `Iter` (sus tipos de elemento no son uniformes). Si implementas `Iter`
para tu propio tipo, se puede recorrer con `doiter` tal cual y pasar a las
[funciones de secuencia](sequences.md#4-funciones-de-secuencia-sobre-iter).

## 2. `Eq` / `Ord` (comparación)

Corresponden a `PartialEq`/`PartialOrd` de Rust (con los nombres `Eq`/`Ord`). Se usan en las restricciones
`where` de las funciones genéricas para exigir que los tipos de los elementos se puedan comparar
(`sort`/`member`/`assoc`, etc.).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; hay que implementarlo
  (not-equals ((self Self) (other Self)) bool             ; implementación por defecto
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; hereda de Eq
  (less ((self Self) (other Self)) bool)                  ; hay que implementarlo
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Para implementar `Eq` solo se escribe `equals`, y para `Ord` solo `less`. Las implementaciones por defecto
completan el resto. `Ord` hereda de `Eq`, así que hace falta `impl Eq X` antes de `impl Ord X`.

Cada método de trait se puede llamar como función tal cual (dentro de una restricción `where (Eq A)`/
`(Ord A)`, o sobre un tipo concreto que lo implemente):

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Si son iguales (el `==` de Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Si no son iguales (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` está implementado para: todos los tipos numéricos (`i8` a `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr` (`eq`, es decir, identidad; lo usan los patrones de
valor de `match`) y `cons-cell<A,B>` (recursivamente, cuando los elementos son `Eq`). `Ord` está
implementado para: todos los tipos numéricos, `char` `string` y `cons-cell<A,B>` (lexicográficamente,
cuando los elementos son `Ord`).

Los nombres de los métodos no coinciden con los operadores incorporados (`= /= < <= > >=`) ni con `eq`/`lt`
porque las funciones incorporadas no se pueden redefinir, y cada implementación delega en ellas. Los
propios operadores de comparación escalares son métodos incorporados de cada tipo receptor
([Números](numbers.md), [Cadenas y caracteres](collections.md)). Dentro de una restricción, escribir los
operadores los lee como los métodos de trait (capítulo 3).

## 3. Traits aritméticos (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Una capa para que el código genérico exija "un tipo que se pueda sumar". **La aritmética sobre tipos
concretos usa los operadores incorporados** ([Números](numbers.md)) y no pasa por esta capa.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; la distancia es siempre int (como en ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; sin métodos; una combinación de seis
```

**Dentro de una restricción se pueden escribir operadores.** Cuando el receptor es una variable de tipo
ligada por `where`, los operadores se leen como métodos de trait (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

El método del trait no se llama `+` porque `+` es el nombre de un método incorporado e `impl` se niega a
redefinirlo (`cannot redefine built-in method`). No hay `Neg`: `(- x)` se expande a `(- (- x x) x)`, así
que basta con `Sub`.

Implementados para: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` en todos los tipos numéricos (excepto
`complex`), y `Bits` en todos los tipos enteros y en `int`.

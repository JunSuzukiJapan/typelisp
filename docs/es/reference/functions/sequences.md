<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Pares, expresiones S y secuencias

El par genérico `cons-cell`, los datos de expresiones S `Sexpr`, los símbolos, las funciones de secuencia
escritas sobre `Iter` y las funciones de orden superior.

## 1. Pares `cons-cell<A,B>`

`cons`/`car`/`cdr` son el constructor y los accesores de campo del **tipo par genérico `cons-cell<A,B>`**
(un `defstruct` de la biblioteca estándar). Los campos se pueden leer como `variable::car`/`variable::cdr`
(la sintaxis de accesores de `defstruct` de la
[Referencia de sintaxis](../syntax.md#36-defstruct--estructuras-tipos-definidos-por-el-usuario)) o como
`(car variable)`/`(cdr variable)`. Para cambiarlos, usa `(setf variable::car v)`/`(setf variable::cdr v)`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Crea un par |
| `car` | `(car p)` | `cons-cell<A,B>→A` | El primer elemento |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | El resto |

`cons-cell` también sirve en lugar de una sintaxis de tuplas. Las funciones de CL que devuelven valores
múltiples (el cociente y el resto de `floor`, el valor y la posición de `read-from-string`, etc.) devuelven
un `cons-cell` en este lenguaje.

## 2. Datos de expresiones S `Sexpr`

El tipo de datos `Sexpr` que devuelve `read` tiene 19 variantes:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` y `array` son datos escritos como `#(..)` y `#nA(..)` ([referencia de
sintaxis](../syntax.md#1-elementos-léxicos)), que contienen un `Vector<Option<Sexpr>>` y un
`Array<Option<Sexpr>>` respectivamente: `len`, `get` y demás funcionan directamente sobre la `v` que
liga `(vector v)`.
`tuple` son datos escritos con `#{..}`, y la `v` que enlaza `(tuple v)` es un
`Vector<Option<Sexpr>>` nuevo con los elementos (para recibir con un solo tipo una tupla de
cualquier longitud).
Las celdas de expresiones S no se manejan con los `cons`/`car`/`cdr` generales del capítulo 1 sino con las
funciones `sexpr-*`. Se usan sobre todo en los cuerpos de `defmacro` para construir y desarmar formas.

**El tipo de los datos de expresiones S es `Option<Sexpr>`.** La lista vacía no es una variante de `Sexpr`
sino el `none` de `Option`, y `Sexpr` en sí significa "una expresión S no vacía". Así que las funciones
`sexpr-*` reciben y devuelven `Option<Sexpr>`.

- `()` es la lista vacía donde se espera un `Option<Sexpr>` (también se puede escribir `(Option::none)`)
- `Sexpr` se amplía implícitamente donde se espera un `Option<Sexpr>` (sin conversión en tiempo de
  ejecución). La dirección contraria, usar un `Option<Sexpr>` como `Sexpr`, afirma "esto no es la lista
  vacía", así que hay que indicarlo explícitamente con `match` o `unwrap`
- En `match`, las 19 variantes de `Sexpr` y `none` se pueden escribir **planas en la misma lista de ramas**
  ([Referencia de sintaxis](../syntax.md#43-match--coincidencia-de-patrones))

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Crea una celda `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | El primer elemento. **La lista vacía para la lista vacía** (como en CL). Panic con un átomo que no es `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | El resto. **La lista vacía para la lista vacía** (como en CL). Panic con un átomo que no es `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Si es un `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Si es la lista vacía |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Si no es un `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Si es un `Sym` (símbolo) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | El contenido de la variante `int` (fixnum o bignum). Panic con otro tipo |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | El contenido de la variante de ese ancho. Panic con otro tipo |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | El contenido de las variantes de coma flotante. Panic con otro tipo |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | El contenido de un `Char`. Panic con otro tipo |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | El contenido de un `Bool`. Panic con otro tipo |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | El contenido de un `Str`. Panic con otro tipo |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | El nombre de un `Sym`. Panic con otro tipo |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Comparación de identidad (`Cons`/`Str` comparan la identidad del objeto, el resto compara valores) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Igualdad estructural (`Cons` recursivamente, `Str` por contenido) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Como `equal`, más comparación sin distinguir mayúsculas y comparación de números entre tipos |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Concatena dos listas `Sexpr` (sin destruirlas). `,@` se expande a esto |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Una lista `Sexpr` nueva con `f` aplicada a cada elemento de una lista `Sexpr` (el `map` del capítulo 4 es para `Iter` y no puede recorrer una lista `Sexpr`) |

Hay nueve accesores numéricos, uno por tipo, porque un `Sexpr` es "el único lugar donde el tipo de un valor
no está escrito en ningún otro sitio". Un `u8` metido en un `Sexpr` entra como la variante `u8` y solo sale
con `(sexpr-u8 s)`. Pasarlo a `(sexpr-int s)` provoca un panic; nunca amplía la respuesta en silencio. Los
enteros de los datos leídos (`'(1 2 3)`, argumentos de macros) son de la variante `int` y se leen con
`(sexpr-int s)`.

Las listas `Sexpr` no tienen operaciones destructivas como `rplaca`/`nconc`. Una celda `Sexpr` no se puede
cambiar tras crearse.

## 3. Símbolos

`symbol` es el tipo de los propios símbolos. Se convierte implícitamente donde se requiere un `Sexpr`, pero
no automáticamente en la otra dirección.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Saca el nombre del símbolo |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Crea un símbolo a partir de una cadena (lo interna) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Si es una palabra clave (`:name`). Los dos puntos forman parte del nombre, así que la comprobación mira el primer carácter ([Referencia de sintaxis](../syntax.md#1-elementos-léxicos)) |

Para `gensym`, consulta [Macros](system.md#8-macros).

## 4. Funciones de secuencia sobre `Iter`

Las funciones de secuencia son **funciones genéricas sobre el trait `Iter`**. De una colección, obtén un
iterador con `(iter coll)` y pásalo (`Vector<T>` / `HashTable<K,V>` / `Array<T>` lo admiten; una lista
`Sexpr` no implementa `Iter`, así que estas funciones no se le aplican). **Una colección resultante se
devuelve como un `Vector` nuevo.** `Iter<A>` en las tablas significa "cualquier implementación de `Iter`
cuyo `Item` sea `A`". Para volver a recorrer el `Vector` devuelto, pasa `(iter result)`.

Funciones que reciben un predicado (corresponden a la familia `-if` de CL):

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Aplicación |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Solo los elementos que cumplen el predicado |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Quita los elementos que cumplen el predicado |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | El primer elemento que cumple el predicado |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | La primera posición que cumple el predicado |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Cuántos cumplen el predicado |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Si todos los elementos cumplen el predicado |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Si algún elemento cumple el predicado (corresponde al `some` de CL; un nombre que no choca con el constructor `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Plegado por la izquierda |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Plegado por la derecha |

Índices, longitud y recortes:

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Número de elementos |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Concatena iteradores. Se pueden dar tres o más |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | El `concatenate` de CL. El tipo del resultado se escribe como **un literal de símbolo citado** (CL usa un especificador de tipo en tiempo de ejecución). `'vector` recibe uno o más, `'string` cero o más (`""` para cero). Las listas `Sexpr` no están incluidas (usa `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Inversión (no destructiva) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | El elemento `n` (`None` fuera de rango) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` con los argumentos al revés |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Los primeros `n` elementos |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` se recorta a la longitud) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | El último **elemento** (no "la última celda" como en CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Todos menos el último elemento |

Funciones que exigen una restricción `Eq` / `Ord` (comparan mediante un trait en lugar de un predicado;
[Traits estándar](traits.md#2-eq--ord-comparación)):

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Si existe un elemento igual a `x` (a diferencia de CL, un `bool`, no el resto de la lista) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | El primer elemento igual a `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | La primera posición igual a `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Cuántos elementos son iguales a `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | El `(sort sequence predicate)` de CL. Una ordenación estable y no destructiva. `cmp` es `true` cuando "el primer argumento va estrictamente antes que el segundo" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | El primer par cuyo `car` es igual a `k`. Saca el valor con `(cdr p)` |

Estas y muchas de las funciones del capítulo 5 también reciben los argumentos de palabra clave de CL `:key`
/ `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count` (capítulo 6).

## 5. El resto de las funciones de secuencia de CL

Todas son funciones genéricas sobre `Iter`, como en el capítulo 4. Las colecciones resultantes se devuelven
como `Vector` nuevos.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Los índices con nombre de CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Todos menos el primero (un `Vector` nuevo, no una cola compartida) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materializa un iterador en un `Vector` (el `copy-seq`/`copy-list` de CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` invertido, seguido de `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copias de `x` (el `make-list`/`make-sequence` de CL). Como con `Vector::new`, el argumento de tipo sale del tipo esperado, así que un `let` desnudo necesita `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Como `member`, un **`bool`** (un iterador no tiene cola que devolver) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Las negaciones de `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Los mismos tipos que las versiones positivas | Versiones con el predicado negado |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Quita por valor |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Quita duplicados. Como en CL, **se conserva la última aparición** (`:from-end true` conserva la primera) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Sustituye por valor / por predicado |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | sobre `Iter<cons-cell<K,V>>` | Las versiones con predicado y del lado del valor de `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Añade un par al principio |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Empareja dos secuencias. Se detiene en la más corta |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | El `mapcar` de CL sobre varias secuencias. Se detiene en la más corta |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Aplicación por sus efectos secundarios |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Aplica y concatena |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Aplica sobre las **colas** sucesivas |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Aplica sobre las colas por sus efectos secundarios (la contrapartida en `maplist` de `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Aplica sobre las colas y concatena (la contrapartida en `maplist` de `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La posición donde aparece `sub` por primera vez. Si el receptor es un `string`, se elige el método de `string` ([Cadenas](collections.md#1-cadenas-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La primera posición donde difieren. `none` si son iguales |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Fusión. CL exige entradas ordenadas; esta ordena la concatenación |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Añade `x` **al principio** si no está |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Operaciones de conjuntos. CL no especifica el orden; aquí es estable, **por orden de primera aparición** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inclusión |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Si es un sufijo / la parte anterior al sufijo. CL pregunta por **estructura compartida**, pero no hay estructura que compartir, así que aquí se pregunta por un sufijo **como valores** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Igualdad elemento a elemento. `Vector<T>` en sí no implementa `Eq` |
| `caar`…`cddddr` | `(cadr p)` | sobre pares anidados | Las 28 funciones de CL. Recorren **pares, no listas**: `cadr` recibe un `cons-cell<A,cons-cell<B,C>>` |

Lo que tiene CL y este lenguaje no: `list*` (no existe la noción de una lista impropia cuya cola se
sustituye), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (ningún tipo puede describir el recorrido
de un árbol heterogéneo de profundidad arbitraria; para un árbol de `Sexpr`, `equal` corresponde a
`tree-equal`), la familia de las listas de propiedades `getf`/`get-properties`/`symbol-plist`/`remprop` (no
hay representación como una lista sin tipo que alterna claves y valores; `assoc` (listas de asociación) o
`HashTable` cumplen el mismo papel) y las funciones que convierten entre `Vector<T>` y listas `Sexpr` (cada
elemento de una lista `Sexpr` puede tener un tipo distinto, así que no se pueden escribir con un único tipo
de elemento `T`).

## 6. Argumentos de palabra clave

Las funciones de los capítulos 4 y 5 reciben las palabras clave de secuencia de CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count`. Todas son **opcionales**.

| Palabra clave | Tipo | Significado |
|---|---|---|
| `:key` | `(fn (A) A)` | Una proyección que se aplica a cada elemento antes de comparar o probar |
| `:test` | `(fn (A A) bool)` | Una prueba de igualdad que se usa en lugar de `equals` de la restricción `Eq`. El primer argumento es **el elemento que se busca**, el segundo es el elemento (tras `:key`), en el mismo orden que CL |
| `:test-not` | `(fn (A A) bool)` | La negación de `:test` |
| `:start` `:end` | `int` | La ventana `[start, end)` que se recorre. Los índices son relativos a la secuencia entera |
| `:from-end` | `bool` | Una búsqueda responde con la **última** coincidencia. Combinado con `:count`, los elementos afectados se toman desde el final |
| `:count` | `int` | El número máximo de elementos a los que afectan las familias `remove` / `substitute` |

Qué función recibe cuál sigue a CL:

| Función | Palabras clave que recibe |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Todas las de arriba (incluida `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (el `:key` de `assoc` se aplica al `car`, el de `rassoc` al `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; quita solo uno, desde el final
(position 3 (iter v) :start 1)                          ; el índice es relativo a la secuencia entera
```

**Diferencias con CL**:

1. **La proyección de `:key` se queda dentro del tipo de los elementos** (`(fn (A) A)`). No puede proyectar
   a otro tipo como en CL: una variable de tipo extra no se podría determinar cuando se omite el argumento.
   Donde haga falta una proyección a un tipo distinto, pasa una lambda a la familia `-if`
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **En las búsquedas por elemento, `:key` se aplica solo a los elementos** (no al elemento que se busca).
   Es la misma regla que en `find`/`position`/`count`/`member`/`remove`/`substitute` de CL. En las
   operaciones de conjuntos ambos lados son elementos, así que se aplica a los dos.
3. **Solo las palabras clave de `search` tienen nombre en lugar de número.** En CL, `:start1`/`:end1` son
   para el **patrón** y `:start2`/`:end2` para la secuencia en la que se busca. En este lenguaje el receptor
   va primero, así que los mismos números significarían lo contrario, y además en silencio. `:start`/`:end`
   son para el receptor y `:sub-start`/`:sub-end` para el patrón, de modo que un `:start1` despistado da un
   error de "palabra clave desconocida". `mismatch` y `replace` tienen el mismo orden de argumentos que CL,
   así que conservan los números de CL.

## 7. Operaciones destructivas

Métodos de `Vector<T>`. **Modifican el receptor y devuelven el propio receptor**, así que `(nreverse v)` se
escribe igual que `reverse` y el propio `v` también queda invertido.

| Nombre | Forma | Descripción |
|---|---|---|
| `nreverse` | `(nreverse v)` | Invierte en su sitio |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versiones en su sitio de `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versiones en su sitio de la familia `substitute` |
| `nbutlast` | `(nbutlast v)` | Descarta el último elemento |
| `fill` | `(fill v x)` | Pone todos los elementos a `x`. La longitud no cambia |
| `replace` | `(replace v src)` | Sobrescribe desde el principio con los elementos de `src`. `(min (len v) (len src))` elementos |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. La misma cuenta que arriba |
| `nconc` | `(nconc v w)` | Añade los elementos de `w` a `v`. A diferencia de CL, **no reescribe estructura compartida** (`w` no se ve afectado) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Sustituye el contenido de `v` por `src` (la longitud también cambia) |
| `rplaca` `rplacd` | `(rplaca p x)` | Reescribe el `car`/`cdr` de un `cons-cell` y devuelve la propia celda |

Palabras clave que reciben:

| Versión destructiva | Palabras clave que recibe |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (el receptor es la `sequence-1` de CL) |

`vector-push-extend`/`vector-pop` son simplemente `push`/`pop` de `Vector<T>`. Un `Vector<T>` siempre
crece, así que no hay nada que corresponda a la distinción de CL entre "un vector con puntero de relleno" y
"un vector simple".

## 8. Funciones de orden superior

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Devuelve su argumento |
| `const` | `(const x y)` | `(A,B)→A` | Devuelve el primer argumento |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Composición de funciones `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Intercambia los argumentos de una función de dos argumentos |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negación de un predicado |

No existe el `constantly` de CL (el tipo del argumento ignorado solo aparecería en el tipo de retorno y no
se podría determinar). Escribe `(lambda ((x T)) A v)`.

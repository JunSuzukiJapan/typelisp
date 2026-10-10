<!-- translated-from: docs/ja/reference/functions/collections.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# Cadenas, caracteres y colecciones

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>` y `Deque<T>`.

## 1. Cadenas `string`

Las cadenas son inmutables.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Convierte a mayúsculas (solo ASCII). Como el `string-upcase` de CL, devuelve una cadena nueva. Las cadenas son inmutables, así que no existe el destructivo `nstring-upcase`; este ocupa su lugar |
| `downcase` | `(downcase s)` | `string→string` | Convierte a minúsculas (solo ASCII). Ocupa el lugar de `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Pone en mayúscula la primera letra de cada palabra y el resto en minúsculas (el `string-capitalize` de CL). Una palabra es una secuencia máxima de letras y dígitos |
| `length` | `(length s)` | `string→int` | Número de caracteres |
| `ref` | `(ref s i)` | `(string,int)→char` | El carácter `i`. Panic fuera de rango |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | La subcadena `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Concatenación. Se pueden dar tres o más (lo mismo que `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Comparación lexicográfica |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Menor estricto lexicográfico (lo mismo que `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Comparación de identidad (si son el mismo objeto, no si tienen el mismo contenido) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Compara contenidos (distingue mayúsculas) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Compara contenidos (sin distinguir mayúsculas, solo ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Si los contenidos difieren (el `string/=` de CL. La forma variádica compara pares adyacentes) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Orden sin distinguir mayúsculas (el `string-lessp` de CL, etc.). Con un prefijo común, la más corta es menor |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Una cadena de `n` copias de `c` (el `make-string` de CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | La posición donde aparece `sub` por primera vez. **El `search` de CL tiene los argumentos al revés** (`(search pattern sequence)`). La cadena vacía se encuentra en 0. Para las palabras clave, consulta [argumentos de palabra clave de las secuencias](sequences.md#6-argumentos-de-palabra-clave) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | La primera posición donde difieren. `none` solo cuando son `equal`. Si una es prefijo de la otra, el final de la más corta. Palabras clave como arriba |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Quita los caracteres contenidos en `bag` de ambos extremos / de la izquierda / de la derecha (el `string-trim` de CL, etc.). Sin `bag`, los espacios en blanco `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Divide en `sep`. CL no tiene equivalente. Los separadores consecutivos producen elementos vacíos. Panic si `sep` está vacío |
| `to-string` | `(to-string x)` | `T→string` | Convierte a cadena como hace `~a`. Implementado para `int`/`i32`/`f64`/`bool`/`char`/`string` (el `princ-to-string` de CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Codifica como UTF-8 (cada elemento 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Decodifica. `none` si no es UTF-8 válido |

## 2. Caracteres `char`

Un `char` es un valor escalar Unicode. La conversión de mayúsculas y la clasificación solo tratan el rango
ASCII.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Convierte a mayúscula (solo ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Convierte a minúscula (solo ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Comparación por punto de código |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Menor estricto por punto de código (lo mismo que `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Si es una letra ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Si es un dígito ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Compara valores |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Compara valores sin distinguir mayúsculas (el `char-equal` de CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Si los valores difieren (el `char/=` de CL. **La forma variádica compara pares adyacentes**, a diferencia de CL, que pregunta si todos los pares difieren) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Orden sin distinguir mayúsculas (el `char-lessp` de CL, etc.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Mayúscula / minúscula / si tiene distinción de mayúsculas (el `upper-case-p` de CL, etc.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Una letra o un dígito (mismo nombre que en CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Si es imprimible. Incluye el espacio, no el salto de línea ni el tabulador (el `graphic-char-p` de CL) |
| `standardp` | `(standardp c)` | `char→bool` | Si es uno de los 96 caracteres estándar de CL, es decir, `graphicp` más el salto de línea (el `standard-char-p` de CL) |
| `char->int` | `(char->int c)` | `char→int` | El valor escalar Unicode (lo inverso es `int->char`/`try-int->char` en [Números](numbers.md#1-enteros-de-ancho-fijo)). Corresponde al `char-code`/`char-int` de CL |
| `char->string` | `(char->string c)` | `char→string` | Una cadena de un carácter. La función `string` de CL cubre esto recibiendo un designador, pero este lenguaje no tiene designadores, así que la dirección va en el nombre |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | El **peso** del dígito en esa base (el `digit-char-p` de CL). `digitp` es otra función que devuelve `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | El carácter de peso `w`. Mayúsculas a partir de 10 (el `digit-char` de CL; la base es como mucho 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | El nombre del carácter. Solo tienen nombre los caracteres con nombre que el lector sabe leer (el `char-name` de CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | El carácter de un nombre. No distingue mayúsculas y también acepta los alias del lector (`linefeed`/`null`) (el `name-char` de CL) |

No hay constante correspondiente a `char-code-limit` (el límite superior de `char` lo fija Unicode, no el
lenguaje).

## 3. `Vector<T>`

Un arreglo que puede crecer.
Un valor se puede escribir `#(1 2 3)` ([referencia de sintaxis](../syntax.md#1-elementos-léxicos);
el tipo de los elementos viene del contexto o del primer elemento, y cada evaluación crea un vector
nuevo). También se imprime como `#(1 2 3)`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Crea un vector vacío. El argumento de tipo sale del tipo esperado, así que en un `let` desnudo escribe `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copias de `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Añade al final |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Lee el elemento `i`. Panic fuera de rango |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Cambia el elemento `i`. Panic fuera de rango. También se puede escribir `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Número de elementos |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Quita el último elemento y lo devuelve. `None` si está vacío (a diferencia de `get`/`set`, no provoca un panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Crea un iterador que implementa `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Añade `x` si no existe un elemento igual (el `pushnew` de CL. No necesita reescribir un lugar, así que es un método y no una macro) |

`map`/`filter` y compañía son [funciones de secuencia](sequences.md#4-funciones-de-secuencia-sobre-iter):
pasa el vector por `iter`, como en `(map (iter v) f)`. Las operaciones destructivas (`nreverse`, `delete`,
etc.) están en [Operaciones destructivas](sequences.md#7-operaciones-destructivas).

## 4. `HashTable<K,V>`

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Crea una tabla vacía |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Búsqueda |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Insertar o sobrescribir |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Quita la entrada y devuelve el valor anterior, si lo había |
| `count` | `(count h)` | `HashTable<K,V>→int` | Número de entradas |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Lo quita todo |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Una instantánea de las claves |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Una instantánea de los valores |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Una instantánea de los pares `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Un iterador que implementa `Iter`. Los elementos son `cons-cell` `(k . v)`. Corresponde al `with-hash-table-iterator` de CL; `doiter`/`map`/`filter` y otros funcionan sobre él tal cual |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | El `maphash` de CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | El `hash-table-size` de CL. En esta tabla es el número de entradas ocupadas (igual a `count`) |

**Cualquier tipo que implemente `Hash` puede ser clave**, incluidos los tipos `defstruct`/`defenum`.
`get`/`set`/`remove` llevan `(where (Hash K))`, así que una tabla cuya clave es un tipo que no lo implementa
es un **error de tipos** (`f64` no tiene `Hash` por culpa de `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; devuelve un valor no negativo que cabe en un fixnum
```

Implementado para: `int` y los seis enteros de ancho fijo, `bool`, `char`, `string` y `symbol` (no para los
números de coma flotante). En tus propios tipos, mantén el resultado no negativo haciendo `logand` con
`*sxhash-mask*` (2^30-1). Para hacer el hash de una cadena, puedes llamar a `(sxhash-string s)` (FNV-1a de
32 bits), que es lo que usa la implementación de `string`.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Si dos claves son la misma lo decide **el propio tipo de la clave** (`sxhash`, y `equals` de `Eq`, el
supertrait de `Hash`), no la identidad de objeto. Por eso, como arriba, se puede buscar con una clave que es
"un valor distinto pero igual".

Que `sxhash` colisione no es problema (el contrato de `Hash` solo va en un sentido: los valores iguales
deben tener el mismo hash). Las claves que colisionan se distinguen con `equals`.

## 5. `Array<T>` (arreglos multidimensionales)

Un `defstruct` de la biblioteca estándar. No es un tipo incorporado, así que con él se puede hacer todo lo
que se puede hacer con un `defstruct`.
Un valor se puede escribir `#2A((1 2) (3 4))` ([referencia de
sintaxis](../syntax.md#1-elementos-léxicos)).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | El `make-array` de CL. `dims` se copia. `init` es el valor inicial de cada celda (el `:initial-element` de CL; este lenguaje no tiene "celda sin enlazar", así que es obligatorio). `:fill-pointer` solo para una dimensión |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | El `aref` / `(setf (aref …))` de CL. Panic si un índice está fuera de rango |
| `aref` | `(aref a i j …)` | — | La escritura de CL con los índices sueltos. Se expande a `get`/`set` de arriba. `(setf (aref a i j) v)` también funciona |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | El `row-major-aref` de CL. Un índice plano |
| `rank` | `(rank a)` | `Array<T>→int` | El `array-rank` de CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | El `array-dimension` de CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | El `array-dimensions` de CL. Devuelve una **copia**, igual que CL devuelve una lista nueva |
| `total-size` | `(total-size a)` | `Array<T>→int` | El `array-total-size` de CL (el número de celdas reservadas, independiente del puntero de relleno) |
| `len` | `(len a)` | `Array<T>→int` | El `length` de CL sobre arreglos. El puntero de relleno si lo hay; si no, `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | El `array-in-bounds-p` de CL. Falso (no un error) incluso cuando el **número** de índices es incorrecto |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | El `array-row-major-index` de CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | El `adjust-array` de CL. El rango no puede cambiar. Los elementos que siguen dentro del rango conservan sus índices y las celdas nuevas reciben `init`. A diferencia de CL, no devuelve el arreglo (todos los arreglos de este lenguaje son ajustables, así que no hay un segundo arreglo que devolver) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | El `vector-push-extend` de CL. Panic sin puntero de relleno |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | El `vector-pop` de CL. `none` si está vacío |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | El puntero de relleno (`none` si no lo hay). Se puede escribir con `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Un iterador en orden por filas. Se detiene en el puntero de relleno si lo hay |

- **Los índices son un `Vector<int>`.** Un método no puede declarar "el mismo tipo de argumento repetido
  cualquier número de veces al final", y la escritura `aref` salva esa distancia.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **no
  existen**. El tipo estático del receptor ya responde a esas preguntas.
- `Array::new` es el constructor en orden de campos que genera `defstruct` y no está pensado para crear
  arreglos. Usa `Array::make`.
- **Los arreglos se imprimen con la sintaxis de arreglos de CL.** El rango 1 es `#(1 2 3)`; los demás rangos
  son `#nA` seguido de tantos niveles de paréntesis (`#2A((1 2 3) (4 5 6))`); el rango 0 es `#0A5`. La
  impresión se detiene en el puntero de relleno si lo hay. Poner `*print-array*`
  ([Impresión](printing.md#6-controlar-cuánto-se-imprime)) a falso imprime solo la forma, `#<array 2x3>`.
  Solo un arreglo cuyos elementos son un `defstruct` sin `print-object` se imprime en la forma incorporada
  `#<array<...> ...>` (no es un error).

## 6. `BitVector` (vectores de bits)

Una secuencia de bits de longitud fija. Un `defstruct` de la biblioteca estándar.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Longitud `n`, todos los bits a 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic fuera de rango |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Las escrituras de CL. `(setf (bit v i) b)` también funciona. El `sbit` de CL solo se diferencia de `bit` en que exige un vector de bits simple, pero este lenguaje solo tiene una clase de vector de bits |
| `len` | `(len v)` | `BitVector→int` | Número de bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Devuelven un vector de bits nuevo. Panic si las longitudes difieren. No hay tercer argumento como en CL (el destino del resultado) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complemento |

No existe `bit-vector-p` (lo responde el tipo estático).

## 7. `HashSet<T>`

Una colección de elementos sin duplicados (el `HashSet` de Rust). Un `defstruct` de la biblioteca
estándar cuyo contenido es un `HashTable<T,()>`. El tipo del elemento debe implementar `Hash`, igual
que una clave de `HashTable`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `make` | `(HashSet::make)` | `()→HashSet<T>` | Crea un conjunto vacío. El argumento de tipo sale del tipo esperado |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | Añade `x`. `true` si no estaba, `false` si ya estaba |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | Si `x` está |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | Quita `x`. `true` si estaba |
| `count` | `(count s)` | `HashSet<T>→int` | El número de elementos |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | Lo borra todo |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | Un iterador sobre los elementos. El orden no está definido |

```lisp
(let ((seen (the HashSet<string> (HashSet::make))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

Común a los tres tipos de los capítulos 7 a 9:

- Se crean con `make`. `new` es el constructor en orden de campos que genera `defstruct`, no el que
  se usa para crearlos (como con `Array::make`).
- `iter` recorre una copia tomada al llamarlo. Si se cambia la misma colección dentro de un
  `doiter`, ese bucle no lo ve.
- Si los tipos de los elementos implementan `print-object`, se imprimen los elementos, en la forma
  `#<hashset "a" "b">` `#<sortedtable 1 "a">` `#<deque 1 2>`.

## 8. `SortedTable<K,V>`

Una tabla en orden ascendente de clave (el `BTreeMap` de Rust). El tipo de la clave debe implementar
`Ord`. Claves y valores se guardan en dos `Vector` en orden de clave, y la búsqueda es binaria.
`set` de una clave nueva y `remove` desplazan los elementos posteriores a su posición.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `make` | `(SortedTable::make)` | `()→SortedTable<K,V>` | Crea una tabla vacía |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | Búsqueda |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | Inserta o sobrescribe |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | Elimina y devuelve el valor antiguo si lo había |
| `count` | `(count t)` | `SortedTable<K,V>→int` | El número de elementos |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | Lo borra todo |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | Las claves, de menor a mayor |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | Los valores, en orden de clave |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | Tuplas `#{clave valor}` en orden de clave |

```lisp
(let ((t (the SortedTable<string,int> (SortedTable::make))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 y pear 3
```

## 9. `Deque<T>`

Una secuencia en la que se puede meter y sacar por ambos extremos (el `VecDeque` de Rust).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `make` | `(Deque::make)` | `()→Deque<T>` | Crea una secuencia vacía |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | Añade al principio / al final |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | Saca el elemento del principio / del final y lo devuelve. `none` si está vacía |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | Mira el elemento del principio / del final (sin sacarlo) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | El `i`-ésimo desde el principio. `none` fuera de rango |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | Sobrescribe el `i`-ésimo. Panic fuera de rango |
| `count` | `(count d)` | `Deque<T>→int` | El número de elementos |
| `clear` | `(clear d)` | `Deque<T>→Unit` | Lo borra todo |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | Desde el principio, en orden |

```lisp
(let ((q (the Deque<int> (Deque::make))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```

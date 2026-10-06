<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Números

Operaciones sobre enteros, números de coma flotante, racionales, números complejos y booleanos, y otras
funciones relacionadas con números. Para leer las formas de llamada, consulta
[Funciones incorporadas](README.md).

## 1. Enteros de ancho fijo

Hay siete tipos enteros: **`int`** (el `integer` de CL: precisión arbitraria, y el tipo por defecto de los
literales enteros sin anotar; capítulo 3), y los de ancho fijo `i8` `i16` `i32` `u8` `u16` `u32`. Para cuál
se resuelve una operación lo decide el tipo del primer argumento (son independientes entre sí, sin
conversiones implícitas). **No hay tipo entero de 64 bits.** Un valor en tiempo de ejecución es una palabra
cuyos bits bajos son una etiqueta, así que solo quedan 63 bits para un entero inmediato, y un tipo que
dijera tener 64 bits tendría que descartar el bit superior en algún sitio. `int` se convierte en bignum al
superar esos 63 bits, así que si el ancho no importa, usa `int`. La tabla de abajo es para los seis tipos de
ancho fijo (la tabla de `int` está en el capítulo 3).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Las cuatro operaciones aritméticas. `/` trunca hacia cero y provoca un panic en la división por cero |
| `mod` | `(mod a b)` | `(T,T)→T` | Resto (el `mod` de CL, **división por defecto (floor)**: el signo sigue al divisor. `(mod -7 3)`→`2`). Panic en la división por cero |
| `rem` | `(rem a b)` | `(T,T)→T` | Resto (el `rem` de CL, **división truncada**: el signo sigue al dividendo. `(rem -7 3)`→`-1`). Panic en la división por cero |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Corresponden a `floor`/`ceiling`/`round`/`truncate` de dos argumentos de CL (`(floor 7 2)`→cociente 3, resto 1). En lugar de valores múltiples, devuelven el cociente y el resto en un `cons-cell` (`car`=cociente, `cdr`=resto). `round-div` redondea los empates al par, como CL |
| `abs` | `(abs x)` | `T→T` | Valor absoluto |
| `signum` | `(signum x)` | `T→T` | Signo (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Máximo común divisor |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Mínimo común múltiplo (0 si alguno es 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | El mayor / el menor (tres o más argumentos se expanden con la escritura variádica del capítulo 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Comparación |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Todos lo mismo que `=` (no hay diferencia para números del mismo tipo) |
| `int->float` | `(int->float x)` | `T→f64` | Conversión ampliadora a `f64` |
| `int->int` | `(int->int x)` | `T→int` | Conversión ampliadora a `int` (siempre exacta). Lo que hace `(as int x)` |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Conversión ampliadora a `ratio` (siempre exacta) |
| `int->char` | `(int->char x)` | `T→char` | Interpreta el valor como un valor escalar Unicode. Panic con un valor no válido |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Una versión de `int->char` que devuelve `None` si falla |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Conversión de ancho. Los valores que no caben se truncan (como el `as` de Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La misma conversión como pregunta. `None` si el valor no cabe en ese ancho |

Estas conversiones son también lo que hacen las formas especiales `(as Type x)`/`(try-as Type x)`
([Referencia de sintaxis](../syntax.md#7-otras-formas-especiales)). Las operaciones de bits
(`logand`/`ash`/`ldb`, etc.) y los predicados (`zerop`/`evenp`, etc.) tienen la misma forma en todos los
tipos, así que están reunidos en los capítulos 11 y 9.

`i8` `i16` `u8` `u16` `u32` tienen exactamente la tabla de este capítulo, y `f32` tiene exactamente la tabla
de `f64` del capítulo 4.

**Un nombre de tipo significa su ancho y su signo, nada más.** `i32` significa "tratar 32 bits como con
signo" y `u32` significa "tratar 32 bits como sin signo". `(+ (the u8 200) (the u8 100))` es `44`,
`(+ 2147483647 1)` (como `i32`) es `-2147483648` y `(lognot (the u32 0))` es `4294967295`. `f32` es igual:
un binary32 de verdad. `(/ (the f32 1.0) (the f32 3.0))` se imprime como `0.33333334`, un valor distinto
del resultado en `f64` `0.3333333333333333`.

El catálogo derivado de CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` y los predicados del capítulo 9) existe
para `int`/`i32`/`f64`/`ratio`. Si lo necesitas para otro ancho, pasa con `(as int x)` / `(as i32 x)` (hay
conversiones de ancho para cada par).

## 2. Palabras en bruto en la frontera con C (`ptr` / `c-long` / `c-ulong`)

Tres tipos usados solo para pasar valores a funciones de C declaradas con
[`defffi`](../syntax.md#33-defffi--declarar-funciones-de-c-ffi) y desde ellas. `ptr` es un puntero opaco, y
`c-long` / `c-ulong` son el `long` / `unsigned long` de C. Para convertir uno en valor hay que estar dentro
de `(unsafe ...)`.

**No hay aritmética.** No se aplica nada de la tabla del capítulo 1: no se pueden escribir ni `(+ p 1)` ni
`(< n m)`. Son palabras para entregar a C, no tipos con los que calcular, así que para calcular hay que
pasar a un tipo con ancho. `c-long` / `c-ulong` solo tienen conversiones:

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Las mismas conversiones de ancho que en el capítulo 1. Los valores que no caben se truncan |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La misma conversión como pregunta |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | La vía de entrada, desde la otra palabra en bruto y desde los tipos enteros del capítulo 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Igual que arriba |
| `int->int` | `(int->int x)` | `T→int` | **Siempre exacta**. La forma honesta de leer un `size_t` que no cabe en un `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` es lo que hacen estas, y las conversiones
existen para cada par con los tipos enteros del capítulo 1. `ptr` ni siquiera tiene esta tabla: no se
ofrece ninguna forma de leer un puntero como número. Es un valor que solo se pasa, se recibe y se entrega a
otra función de C.

**Tampoco se pueden imprimir.** `(println "~a" x)` no acepta una palabra en bruto (no tiene representación
`Sexpr`), así que primero pásala a un tipo con ancho, como en `(println "~a" (as int n))`.

"No hay tipo entero de 64 bits", del principio del capítulo 1, se cumple también para estos tres. Se cumple
**porque no se pueden almacenar**: no pueden ser un campo de `defstruct`, un `defvar`, ni ir dentro de un
argumento de tipo o de un `Sexpr`, así que son palabras que solo atraviesan una función como argumentos,
valores de retorno y variables locales. Para los detalles, consulta la
[Referencia de sintaxis](../syntax.md#ptr--c-long--c-ulong--palabras-de-máquina-en-bruto).

## 3. Enteros de precisión arbitraria `int`

El `integer` de CL, y el **entero** de este lenguaje: los literales enteros sin anotar tienen este tipo, y
las funciones incorporadas que devuelven un número, como `length` y `char->int`, devuelven este tipo. Un
valor se guarda como valor inmediato de 63 bits (fixnum) mientras cabe, se promueve automáticamente a
bignum cuando el resultado de una operación deja de caber, y vuelve a ser inmediato cuando vuelve a caber.
`eq` es siempre identidad de valor dentro del rango fixnum, y `eql`/`=` son identidad numérica en todo el
rango. Es un tipo distinto de los tipos enteros de ancho fijo (capítulo 1), sin conversión implícita:
`(as int x)` es la ampliación exacta desde un ancho fijo, y `(as i32 n)` / `(try-as i32 n)` son el
truncamiento / la comprobación desde `int` (el mismo significado que `int->W` / `try-int->W` del capítulo 1).

La variante entera de `Sexpr` también es simplemente `int` (`(int n)` acepta tanto fixnums como bignums).

Las funciones incorporadas que reciben un índice o una cuenta (`substring`, `get` de `Vector`, la cantidad
de desplazamiento de `ash`, etc.) aceptan `int`, pero pasar un valor que no cabe en un fixnum es un error en
tiempo de ejecución ("an integer argument does not fit a fixnum").

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Nunca desbordan (promueven) |
| `/` | `(/ a b)` | `(int,int)→int` | Trunca hacia cero. Panic en la división por cero |
| `mod` | `(mod a b)` | `(int,int)→int` | Resto de la división por defecto (el signo sigue al divisor) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Todos `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Igual que en el capítulo 11 (complemento a dos con infinitos bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Igual que en el capítulo 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Truncamiento / comprobación. `W` es uno de los seis anchos o `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identidad (en el lado de ancho fijo y de palabras de C, `int->int` amplía; capítulo 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | La misma forma que en el capítulo 1. `expt` solo acepta exponentes no negativos |

## 4. Números de coma flotante (`f64` / `f32`)

`f32` tiene la misma tabla.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. La división por cero no provoca un panic; da `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Resto de la división por defecto (como en CL; el signo sigue al divisor. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Resto de la división truncada (como en CL; el signo sigue al dividendo. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Comparación |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Todos lo mismo que `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potencia |
| `abs` | `(abs x)` | `f64→f64` | Valor absoluto |
| `signum` | `(signum x)` | `f64→f64` | Signo (`1.0`/`-1.0`; `±0.0`/`NaN` se devuelven tal cual. Como en CL, a diferencia del `signum` de Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | El mayor / el menor (tres o más argumentos se expanden con la escritura variádica del capítulo 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Operaciones unarias |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Funciones trascendentes. `log` es el logaritmo natural |
| `log` (dos argumentos) | `(log x base)` | `(f64,f64)→f64` | Logaritmo en una base dada. Se expande a `(/ (log x) (log base))` (capítulo 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Corresponden a las versiones de dos argumentos de CL (`(floor 7.0 2.0)`→cociente 3, resto 1). El mismo diseño que las funciones homónimas del capítulo 1 (`car`=cociente, `cdr`=resto) |
| `float->int` | `(float->int x)` | `f64→int` | Convierte a `int` truncando hacia cero (el `truncate` de CL; exacto para valores finitos de cualquier tamaño). Panic con infinito y NaN. Para un ancho fijo, usa `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Convierte a `ratio` como el racional binario exacto (el `rational` de CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Convierte entre anchos de coma flotante. `float->f32` redondea al más cercano, `float->f64` es siempre exacta. Lo que hace `(as f32 x)` |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | La misma conversión como pregunta. `none` si el redondeo cambia el valor (ampliar a `f64` es siempre `some`). Lo que hace `(try-as f32 x)` |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Las funciones homónimas de CL. Alias de `floor`/`ceiling`/`round`/`truncate` de arriba: en CL las que no llevan prefijo devuelven enteros, así que las que llevan `f` coinciden con el comportamiento de este lenguaje |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 2 / 53 / 53 respectivamente (solo la precisión de `0.0` es 0). `f64` es siempre IEEE-754 binary64, así que son constantes |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` o `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | La mantisa (en `[1/2,1)`, sin signo) y el exponente. CL devuelve tres valores, pero no hay valores múltiples, así que el signo se deja a `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | La misma descomposición con una mantisa entera exacta de 53 bits. `mantisa * 2^exponente` es exactamente el valor original |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **El racional más simple que se vuelve a leer como ese número de coma flotante** (`(rationalize 0.1)` es `1/10`). Para el valor binario exacto, usa `float->ratio` |

**Diferencia con CL: cómo redondea `round`.** `round` (y por tanto `fround`/`round-div`) redondea
**alejándose de cero** (`(round 2.5)` = `3.0`). CL redondea **al par**, dando `2`.

## 5. Racionales `ratio`

Racionales de precisión arbitraria compatibles con CL. Se mantienen siempre en su forma irreducible con
denominador positivo, y se reservan en el montículo. No hay conversión implícita con los tipos enteros ni
con `f64` (usa un método de conversión explícito o `as`/`try-as`). Para la sintaxis de los literales de
ratio, consulta la [Referencia de sintaxis](../syntax.md#1-elementos-léxicos).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Las cuatro operaciones (resultados siempre irreducibles). `/` provoca un panic en la división por cero |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Resto de la división por defecto (como en CL; el signo sigue al divisor) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Resto de la división truncada (como en CL; el signo sigue al dividendo) |
| `abs` | `(abs x)` | `ratio→ratio` | Valor absoluto |
| `signum` | `(signum x)` | `ratio→ratio` | Signo (devuelve `1`/`-1`/`0` como `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potencia. El exponente debe ser un `ratio` de valor entero (si no, panic). Un exponente negativo da el recíproco |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | El mayor / el menor |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` no tiene operaciones de bits (en CL son solo para enteros) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Comparación |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Todos lo mismo que `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Numerador en forma irreducible (mismo nombre que en CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Denominador en forma irreducible (siempre positivo) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Parte entera (truncada hacia cero) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Convierte a `f64` |

Las vías de entrada desde los enteros de ancho fijo y `f64` son `int->int`/`int->ratio` (capítulo 1) y
`float->int`/`float->ratio` (capítulo 4). `int`/`ratio` son tipos separados, independientes de `i32` y los
demás, y la aritmética mixta necesita conversiones explícitas.

## 6. Números complejos `complex`

Una estructura (`defstruct`) de la biblioteca estándar.

**Dos diferencias con CL** (ambas derivan del tipado estático):

1. **Los componentes son siempre `f64`.** Un complejo de CL también puede contener racionales, y
   `(complex 1 2)` y `(complex 1.0 2.0)` son tipos distintos. Un tipo estático tiene que elegir uno, y las
   funciones trascendentes devuelven la clase de coma flotante.
2. **`(sqrt -1.0)` es el `sqrt` real (NaN).** En CL, `sqrt` puede devolver un complejo a partir de un real,
   pero el `sqrt` de `f64` tiene que devolver un `f64`. Un resultado complejo sale de un argumento complejo:
   `(sqrt (complex -1.0 0.0))` es `i`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Construcción. Los componentes se pueden leer directamente como `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Parte real y parte imaginaria. **También funcionan con números reales** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), como en CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Conjugado (también funciona con reales) |
| `phase` | `(phase z)` | `complex→f64` | Argumento en (-pi,pi] (también funciona con reales) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Valor absoluto. **El único `abs` que no devuelve el tipo del receptor** (como en CL, el valor absoluto de un complejo es real) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Aritmética compleja |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Igualdad componente a componente. También implementa `Eq` (no hay `Ord`: los complejos no tienen orden, y el `<` de CL también los rechaza) |
| `zerop` | `(zerop z)` | `complex→bool` | Si ambos componentes son 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` dan valores principales |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | El ángulo del vector `(x,y)`. **El `(atan y x)` de dos argumentos de CL es una escritura de esto** (ramifica según el número de argumentos, como el `log` de dos argumentos) |

Implementa `print-object`, así que `~a`/`~s` lo imprimen como `#C(re im)`, como hace CL (el lector de este
lenguaje no tiene la sintaxis `#C` para volver a leerlo).

## 7. Booleanos

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negación |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Todos comparan la igualdad de valores |

`and`/`or` necesitan evaluación en cortocircuito, así que son formas especiales
([Referencia de sintaxis](../syntax.md#4-enlaces-y-condicionales)).

## 8. Auxiliares numéricos y escrituras de llamada

`abs`/`signum` (todos los tipos numéricos), `gcd`/`lcm` (solo tipos enteros), `rem` (todos los tipos reales,
incluido `f64`) y `expt` (`int`/`f64`/`ratio`) están definidos como métodos de cada tipo numérico (se
resuelven por el tipo del receptor: `(abs x)` es el método del tipo de `x`). Los detalles de cada tipo están
en los capítulos 1, 3, 4 y 5. Los enteros de ancho fijo no tienen `expt` (no tienen promoción y
desbordarían; pasa a `int` con `(as int x)` y usa su `expt`).

### 8.1 Formas variádicas y de 0/1 argumentos

La aritmética y la comparación de CL son variádicas, pero los métodos se resuelven solo por el tipo del
receptor, no por el número de argumentos. Así que **el comprobador expande las siguientes formas en
llamadas de dos argumentos**.

| Forma que puedes escribir | Expansión | Se aplica a |
|---|---|---|
| `(op a b c ...)` | El plegado por la izquierda `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` con cada término ligado a un temporal | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Los de arriba que tienen elemento neutro |
| `(op x)` | Para `+ * max min logand logior logxor`, el propio `x`. `(- x)` cambia de signo, `(/ x)` da el recíproco, `(gcd x)`/`(lcm x)` dan `(abs x)` (como en CL) | Igual que arriba |
| `(cmp x)` | Evalúa `x` y da `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Cada término se evalúa exactamente una vez, de izquierda a derecha (por eso las comparaciones variádicas
pasan por temporales). La forma variádica de `/=` compara **pares adyacentes**, a diferencia de CL, que
pregunta si todos los pares difieren.

### 8.2 `isqrt` y `expt` entero

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | El mayor entero que no supera la raíz cuadrada. Panic con un valor negativo |
| `expt` | `(expt n e)` | `(T,T)→T` | Potencia (por cuadrados). CL devuelve un racional para un exponente negativo, pero un tipo entero no lo puede representar, así que provoca un panic; convierte primero a `ratio` |

## 9. Predicados

| Nombre | Forma | Tipo | Tipos |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (solo tipos enteros, como en CL) |

**No hay predicados de tipo** como `numberp`/`integerp`/`floatp` de CL. Con tipado estático, el tipo de un
valor ya está fijado sin preguntar en tiempo de ejecución.

## 10. Constantes

| Nombre | Tipo | Valor |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Códigos de operación que se pasan a `boole` (en lugar de las palabras clave de CL) |

Constantes de límites numéricos (CLHS 12.1.4.2 / 12.1.3):

| Nombre | Tipo | Descripción |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | El límite superior / inferior de un valor inmediato de 63 bits (2^62-1 / -2^62). Un `int` más allá se convierte en bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Los valores finitos mayor / menor |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | La menor magnitud no nula, incluidos los subnormales |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Lo mismo, limitado a números normalizados |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Siguen la definición de CL (el menor `e` positivo con `(/= (+ 1 e) 1)`), así que son **un ULP mayores que** 2^-53: el propio 2^-53 vuelve a `1.0` con redondeo al par más cercano |

## 11. Operaciones de bits

Definidas sobre complemento a dos con infinitos bits (CL 12.10). Están implementadas para los tipos enteros
de ancho fijo y para `int`, no para `ratio` (CL también tiene operaciones de bits solo para enteros).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Y, O y O exclusivo bit a bit (versiones variádicas y de cero argumentos en 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Complemento bit a bit |
| `ash` | `(ash x count)` | `(T,int)→T` | Desplazamiento aritmético. A la izquierda si `count` es positivo, a la derecha si es negativo |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Si el bit `index` está activo (**el orden de los argumentos es el inverso al de CL**; ver abajo) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | El número de bits activos (para un número negativo, el número de bits a 0) |
| `integer-length` | `(integer-length x)` | `T→T` | El número de bits necesarios para representarlo, sin contar el signo |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Los siete restantes, compuestos a partir de los anteriores |

**Solo el segundo argumento de `ash` es `int` en lugar de `T`.** Es una **distancia** en bits, no un valor del
tipo del receptor, así que el ancho y el signo del receptor no dicen nada sobre la distancia (por la misma
razón por la que `count` en el `(ash integer count)` de CL es cualquier entero). Desplazar a la derecha un
valor sin signo es un desplazamiento lógico (`(ash (the u8 200) -3)` = `25`), y uno con signo es un
desplazamiento aritmético que redondea hacia menos infinito (`(ash (the i32 -100) -4)` = `-7`). El `index`
de `logbitp` es `int` por la misma razón.

**Especificadores de byte.** En lugar del objeto opaco que devuelve el `byte` de CL, se usa un
`cons-cell<int,int>` (`car`=tamaño, `cdr`=posición). Tanto el tamaño como la posición son números de bits,
así que son `int` sea cual sea el ancho del entero que se desarma.

**El entero es el primer argumento, en un orden distinto al de CL.** CL escribe `(ldb bytespec integer)`,
pero este lenguaje elige un método por el tipo del receptor (el primer argumento), y con el especificador
primero no podría elegir por el tipo del entero. Todas las demás operaciones de bits tienen la forma
`(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), y solo la familia `ldb` y `logbitp`
iban al revés, así que se alinearon. Los argumentos restantes mantienen el orden relativo de CL, así que
`(dpb newbyte spec n)` pasa a ser `(dpb n newbyte spec)`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Crea un especificador de byte |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Saca un componente |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extrae de `x` el byte especificado, justificado a la derecha |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Si hay algún bit activo en el byte especificado |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Borra todo lo que está fuera del byte especificado (conservando posiciones) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Deposita el `newbyte` justificado a la derecha en el byte especificado de `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | La versión de `dpb` que conserva posiciones |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Una de las 16 operaciones lógicas de dos operandos, elegida por `op` (una constante `boole-*` del capítulo 10) |

`T` es un tipo que implementa el trait `Bits`, es decir `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Solo
`boole` mantiene `op` primero, ya que ahí no hay motivo para cambiar el orden de CL.

## 12. Números aleatorios

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Un número aleatorio desde `0` hasta `n` sin incluirlo. Si se omite el estado, lo saca de `*random-state*` y lo hace avanzar |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Sin argumento, un estado nuevo; con uno, una copia de él (la copia reproduce la misma secuencia) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Siempre `true` (el tipo estático ya descarta otros tipos; existe solo para corresponderse con CL) |
| `*random-state*` | — | `random-state` | El estado por defecto de `random`. Una global que se puede asignar (sustitúyela con `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | El estado que designa el entero. La misma semilla reproduce siempre la misma secuencia |

El generador es xorshift64 y devuelve la misma secuencia tanto interpretado como compilado.

Un estado nuevo de `make-random-state` se siembra a partir del reloj de pared, así que no se puede
reproducir entre ejecuciones. Para reproducirlo, usa `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; imprime los mismos tres números en cada ejecución
```

**CL no tiene una forma portable de dar una semilla** (`make-random-state` solo acepta `nil`/`t`/un estado),
así que este nombre sigue el `sb-ext:seed-random-state` de SBCL y no a CL.

Semillas distintas dan secuencias distintas. `(seed-random-state 0)` y `(seed-random-state 1)` dan
secuencias distintas, y también `-7` y `7`.

<!-- translated-from: docs/ja/reference/syntax.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Referencia de sintaxis de typelisp

typelisp es un Lisp con tipado estático que se escribe en expresiones S. Para la lista de funciones y métodos
incorporados, consulta [Funciones incorporadas](functions/README.md); para la lista de tipos,
[types.md](types.md); y para leer los mensajes de error, [errors.md](errors.md).

## 1. Elementos léxicos

- **Distingue mayúsculas y minúsculas.** `foo` y `Foo` son símbolos distintos. Los nombres de tipo empiezan por mayúscula (`Option`, `Vector`, etc.; los tipos primitivos como `int` y `string` van en minúsculas).
- **Comentarios**: desde `;` hasta el final de la línea (comentarios de línea). `#| ... |#` (comentarios de
  bloque, que se pueden anidar).
- **Evaluación en tiempo de lectura**: `#.(expr)` **ejecuta la forma que sigue mientras lee** y trata su valor
  como lo leído. Es el único lugar donde el lector es algo más que una función del texto. Hasta dónde puede
  llegar depende del camino de lectura, como en CL:
  - `(load ...)` y el REPL evalúan una forma cada vez, así que puede llamar a **funciones definidas antes en
    el mismo texto** (el `load` de CL).
  - Un archivo de módulo se comprueba como una unidad y lo ejecuta quien lo usa con `use`, así que `#.` solo
    puede alcanzar la biblioteca estándar y lo que la sesión ya ha ejecutado. Ni las propias definiciones del
    archivo ni las de los módulos que usa **se han ejecutado todavía** (igual que el `compile-file` de CL
    necesita `eval-when`).
  - `read` / `read-from-string` dentro de un programa también evalúan `#.` (como en CL).
  - Poner `*read-eval*` (por defecto `true`) a `false` hace de `#.` un error de lectura en todas partes: un
    interruptor para evitar que un texto leído como datos ejecute código (como en CL). Se consulta en cada
    `#.`, así que un `setf` surte efecto a partir de la siguiente forma leída. Dentro de
    `with-standard-io-syntax` es `true`.
- **Booleanos**: `true` / `false`.
- **Enteros**: decimales (`42`, `-7`). Puede ir primero un signo `+`/`-`. Las demás bases se escriben con la
  sintaxis de base de CL `#b`/`#o`/`#x`/`#NNr` (el signo va tras la marca: `#x-ff`). El prefijo `0x` no
  existe en CL y no se adopta: `0xff` se lee como un símbolo.
  Un literal entero sin anotación de tipo es `int` por defecto (precisión arbitraria,
  [Números](functions/numbers.md#3-enteros-de-precisión-arbitraria-int)), sin límite superior de tamaño.
  **Si el tipo esperado es un tipo entero de ancho fijo, el literal toma ese tipo, y se comprueba que el tipo
  puede contener el valor**: `(the u8 300)` es un error de tipos (si quieres recortarlo, escribe
  `(as u8 300)`). `(the u32 4294967295)` y `(the u32 #xFFFFFFFF)` se pueden escribir gracias a esta regla. Si
  un valor `int` cabe en un inmediato de 63 bits o se convierte en bignum lo decide su tamaño, sin sintaxis
  especial (como en CL).
- **Números de coma flotante**: los que contienen un punto decimal o un exponente (`e`/`E`) (`1.5`,
  `3.0e10`). `f64` por defecto (`f32` si ese es el tipo esperado).
- **Ratios**: `numerador/denominador` (por ejemplo `1/3`; también con prefijo de base, como `#x1/F`). Se reducen al leerse, como especifica
  CL (`2/4` es `1/2`). Los que tienen valor entero (`4/2`, etc.) se leen como `int`, no como `ratio`. Un
  denominador cero (`1/0`) es un error de lectura.
- **Caracteres**: `#\` seguido de un carácter o de un nombre de carácter. Por ejemplo `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (también `#\Null`) `#\Backspace`. Los nombres no
  distinguen mayúsculas.
- **Cadenas**: `"..."`. Los escapes son `\n` `\t` `\r` `\0` `\\` `\"` (cualquier otro `\x` es simplemente `x`).
- **Símbolos**: cualquier token que contenga letras, dígitos y símbolos (`+` `<=` `my-func`, etc.).
  `]` y `}` terminan un token, así que no pueden aparecer dentro de un símbolo, y encontrar uno al
  comienzo de un dato es un error de lectura. `[` y `{` sí pueden aparecer dentro de un símbolo:
  como en CL, quedan libres para que el programador los use en [macros de
  lectura](#11-macros-de-lectura-readtable).
  Lo que hay entre `|` y `|` se lee tal como está escrito, delimitadores y dos puntos incluidos (el escape
  múltiple de CL): `|:foo|` es un símbolo ordinario cuyo nombre es `:foo`, no una palabra clave.
- **Palabras clave**: símbolos que empiezan por dos puntos, como `:name` (como en CL). Se escriban en el módulo
  que se escriban, son el único símbolo de ese nombre en el espacio de nombres de las palabras clave, `keyword`
  (el paquete `KEYWORD` de CL). Se evalúan a sí mismos: no buscan ningún enlace y su valor son ellos mismos, con
  tipo estático `symbol`. Las palabras clave con el mismo nombre son siempre el mismo objeto (`(eq :foo :FOO)`
  es verdadero; como los demás símbolos, pasan a minúsculas). Los dos puntos no forman parte del nombre:
  `(symbol->string :foo)` es `"foo"`. `~s` imprime `:foo` y `~a` imprime `foo`. Una palabra clave es una
  constante cuyo valor es ella misma, así que no se puede ligar ni asignar como variable, ni ser el nombre de
  una definición (un error de tipos). Para crearla a partir de una cadena se escribe `(intern "foo" "keyword")`.
  Se comprueba con `keywordp` (si pertenece al espacio de nombres de las palabras clave). Unos dos puntos solos
  `:` o con dos puntos adicionales como `:a:b` son un error de lectura. Los que empiezan por `::` no son
  palabras clave sino rutas absolutas (abajo).
  Ten en cuenta que `:dyn` es una palabra clave reservada solo para posiciones de tipo; escribirla en
  cualquier otro lugar es un error (consulta el [capítulo 2](#2-escritura-de-tipos)).
- **Listas**: `(a b c)`. También se pueden leer pares con punto `(a . b)`.
- **Vectores**: `#(1 2 3)` (como en CL). El contenido son solo literales y no se evalúa: la `a` de
  `#(a b)` es un símbolo, no una variable. El tipo de los elementos viene del contexto
  (`(the Vector<i32> #(1 2))`), o del primer elemento si no hay contexto (`#(1 2 3)` es un
  `Vector<int>`). Todos los elementos deben tener el mismo tipo: `#(1 "a")` es un error de tipo,
  igual que `#()` sin elementos ni contexto. Cada evaluación crea un vector nuevo. Donde se esperan
  datos de expresiones S (`(the Option<Sexpr> #(1 x))`, `'#(..)`, lo que devuelve `read`), es un
  `Vector<Option<Sexpr>>` cuyos elementos son todos datos: la variante `vector` de `Sexpr`.
- **Arrays**: `#2A((1 2) (3 4))` (como en CL). El número entre `#` y `A` es el rango, y esa misma
  cantidad de primeros niveles de anidamiento de listas del contenido son las dimensiones. `#0A x`
  es un array de dimensión cero que contiene un elemento. Listas del mismo nivel con longitudes
  distintas son un error de lectura. El tipo se decide igual que en los vectores y es un `Array<T>`
  (sin elementos, el contexto tiene que darlo, como en `(the Array<f64> #2A(()))`). Como dato de
  expresiones S es un `Array<Option<Sexpr>>`: la variante `array` de `Sexpr`.
- **Tuplas**: `#{"foo" 123 45.6}`. El contenido son todo literales y no se evalúa (la `a` de
  `#{a b}` es un símbolo). El tipo de cada elemento lo decide el contexto, posición por posición
  (`(the #{i32 f64} #{1 2.0})`), y sin contexto es el tipo propio del elemento (`#{"foo" 123}` es
  `#{string int}`). Una tupla tiene de 1 a 12 elementos. Cada evaluación crea una tupla nueva. Para
  construir una a partir de valores calculados, se usa `(tuple a b)`. Donde se esperan datos de
  S-expresión, es una tupla cuyos elementos son todos datos: la variante `tuple` de `Sexpr`.
- **Atributos**: `#[test]` es una marca que se une a la forma siguiente (escrita como en Rust). El único atributo por ahora es `test`, y va sobre un `defun` ([3.16 `#[test]`](#316-test--funciones-de-prueba)). Leído con `read`, se convierte en la lista `(%internal::attribute (test) form)`.
- **La lista vacía `()`**: según el contexto, el valor del tipo `Unit` o el `none` de `Option<Sexpr>`.
  **`Sexpr` no tiene variante de lista vacía**: `Sexpr` significa "una expresión S no vacía", y el tipo de los
  datos de expresiones S es `Option<Sexpr>` (consulta "Patrones para `Option<Sexpr>`" en
  [4.3 match](#43-match--coincidencia-de-patrones)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (solo tiene sentido dentro de una cuasicita)
  - `,@x` → `(unquote-splicing x)` (se empalma como elementos de lista al expandir)
- **Rutas `::`**: `foo::bar` se lee como una ruta a través de módulos, tipos y miembros (no como un único
  nombre de símbolo). Una que empieza por `::`, como `::foo`, es una ruta absoluta desde la raíz. Un `::`
  dentro de argumentos genéricos (`Vec<a::b>` y similares) no se trata como separador de ruta.

## 2. Escritura de tipos

En el código fuente, los tipos se escriben como símbolos o listas normales.

- **Tipos primitivos**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` es el tipo entero (el integer de CL, que pasa automáticamente entre inmediatos de 63 bits y
  bignums; [Números](functions/numbers.md#3-enteros-de-precisión-arbitraria-int)), y los seis tipos de ancho
  fijo se nombran por su ancho y su signo (no hay tipo entero de 64 bits; consulta
  [Números](functions/numbers.md#1-enteros-de-ancho-fijo)).
- **Los tipos racionales**: `ratio` (racionales reducidos) y `rational` (un `int` o un `ratio`, el
  rational de CL). `/` sobre dos enteros y la aritmética que mezcla un `int` con un `ratio` dan un
  `rational`, y los valores `int`/`ratio` se pasan tal cual donde se espera un `rational`. No hay
  conversión implícita con los enteros de ancho fijo ni con `f64` (hágala explícita con `as`/`try-as` o
  un método de conversión; vea [Números](functions/numbers.md#5-racionales-ratio)).
- **Palabras en bruto en la frontera con C**: `ptr` (un puntero opaco), `c-long` / `c-ulong`. Solo para el
  FFI: convertir una en valor requiere `(unsafe ...)`, y los lugares donde pueden aparecer están limitados
  ([3.3 defffi](#ptr--c-long--c-ulong--palabras-de-máquina-en-bruto)). No las uses donde quieras un entero de
  64 bits: no tienen aritmética.
- **Tipos mutables opacos**: `random-state` (el estado de un generador de números aleatorios). No puede ir en
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (sí en `Option<T>`/`Result<T,E>`).
- **El tipo Unit**: `()`
- **El tipo Never**: `!` (el tipo de las expresiones que divergen, como `panic`/`unreachable`/`todo`/un bucle
  que nunca retorna. Encaja con cualquier tipo esperado)
- **Tipos de función**: `(fn (tipos-de-argumentos...) tipo-de-retorno)`. El tipo de una función con argumentos
  variádicos es `(fn (tipos-de-argumentos... &rest tipo-de-elemento) tipo-de-retorno)`.
- **Tipos genéricos**: `Name<T1,T2,...>`. Por ejemplo `Option<i32>` `Result<i32,ParseIntError>`
  `HashTable<string,i32>` `Vector<T>`. Cuando un tipo sigue a un `<` justo después de un nombre, el
  lector lee hasta el `>` correspondiente como argumentos de tipo. Dentro de ellos, espacios y
  saltos de línea pueden separar las partes (`HashTable<string, int>`), y pueden escribirse el tipo
  unit `()` (`Result<(), FileError>`), `:dyn Trait` y tipos tupla. Si no sigue ningún tipo al `<`,
  ese `<` es parte del nombre (`<=`, `string<`). Unos argumentos de tipo que no se cierran con `>`,
  o que no tienen forma de tipo, son un error de lectura (`(a<b c)` también es un error, en los
  argumentos de tipo `b c`). `()` también puede usarse como tipo de campo o de argumento.
- **La forma de aplicación de los tipos genéricos**: `(Name T1 T2 ...)`, una escritura en forma de
  lista que nombra el mismo tipo que `Name<T1,T2,...>`. Por ejemplo `(vector char)` es lo mismo que
  `Vector<char>`. La forma de nombre es la habitual; esta forma **existe para cuando un argumento de
  tipo no puede escribirse dentro de un nombre**: un argumento de tipo es en sí una expresión de
  tipo, pero dentro de `<..>` solo pueden escribirse nombres, `()`, `:dyn` y tipos tupla, no tipos
  función (`Vector<(fn (i32) i32)>` es un error de lectura; si se da nombre al tipo función con
  `deftype`, puede escribirse `Vector<F>`). También puede aparecer en esta forma cuando la
  implementación muestra un tipo, como el resultado de sustituir el tipo asociado de un trait en una
  firma.
- **Tipos tupla**: `#{int string}` (la misma forma que los valores). Una tupla tiene de 1 a 12
  elementos, y un tipo tupla también puede ser argumento de tipo (`Vector<#{int string}>`). Los
  elementos se leen con `t::0` `t::1` y se cambian con `(setf t::0 v)`. `Eq`, `Ord` (comparando
  desde el primer elemento), `Hash` y `print-object` están disponibles cuando todos los tipos de
  elemento los implementan.
- **Nombres de tipo calificados**: se pueden calificar con `::`, como en `module::Type`.
- **Tipos de objeto trait**: `:dyn Trait` (dos palabras separadas por un espacio que forman un tipo).
  Representa un valor cuyo tipo concreto se decide en tiempo de ejecución; las llamadas a métodos de trait
  pasan por una vtable (despacho dinámico). Para un trait con tipos asociados, estos se fijan por posición en
  el orden de declaración (`:dyn Iter<i32>` fija `Item` a `i32`). También se puede escribir dentro de
  argumentos genéricos: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Los valores concretos se
  meten en una caja automáticamente en las posiciones esperadas; la forma explícita es `(as :dyn Trait expr)`.
  Un valor de `:dyn Sub` se puede pasar tal cual donde se exige un `:dyn Super` de cualquiera de sus
  supertraits (todo lo que hereda, transitivamente) (conversión ascendente). No se puede pasar a un trait no
  relacionado. Para las condiciones que debe cumplir un trait para usarse con `:dyn`, consulta
  [3.9 deftrait / impl](#39-deftrait--impl--traits). Escribir `:dyn` fuera de una posición de tipo es un error.
- Tipos genéricos incorporados: `Option<T>` (`(some T)` / `none`), `Result<T,E>` (`(ok T)` / `(err E)`),
  `HashTable<K,V>`, `Vector<T>` y los tipos de concurrencia `Task<T>` / `Thread<T>` / `Chan<T>`
  ([capítulo 12](#12-concurrencia-tareas)). También está `Sexpr`, el tipo de los datos de expresiones S. Los
  tipos de error concretos incorporados son `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, y la biblioteca estándar tiene las estructuras `SimpleError` / `WrappedError`
  (`Error` no es un tipo sino un trait: úsalo como `:dyn Error`). La lista está en [types.md](types.md).
- **Los tipos y los traits comparten un espacio de nombres** (como en Rust): dentro de un módulo, un tipo
  (`defstruct`/`defenum`) y un trait (`deftrait`) no pueden tener el mismo nombre.

## 3. Definiciones de nivel superior

### 3.1 defun — definición de funciones

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Los tipos de los argumentos y el tipo de retorno son obligatorios.
- Una función genérica escribe sus parámetros de tipo entre corchetes angulares tras su nombre:
  `(defun name<T1,T2...> (params) Ret body...)` (la misma sintaxis de corchetes angulares que `Vector<T>` en
  posiciones de tipo).
- `defun`/`lambda`/métodos de `impl` aceptan argumentos variádicos cuando se escribe `&rest (name Type)` al final:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (en el cuerpo, `xs` siempre queda ligado como un
  `Option<Sexpr>`, una lista de expresiones S. Cada argumento real de la llamada se comprueba individualmente
  como `Type2` y después se envuelve en un `Sexpr`).
  `defmacro` también tiene su propio `&rest`, pero se diferencia en que siempre es un `Sexpr` sin tipo
  (`defun`/`lambda` indican el tipo de los elementos). Un tipo de función también puede describir una función
  variádica, como `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (para `defun` y métodos de `impl`; no para `lambda`/`labels`, por el motivo que se da
  abajo, y `defmacro` tiene una implementación aparte, también abajo). El orden es el de CL:
  `obligatorios &optional &rest &key`. Cada parámetro se escribe `(name Type)` o
  `(name Type expr-por-defecto)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; sin valor por defecto
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> en el cuerpo

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; con valor por defecto
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; quien llama escribe `:name valor`, en cualquier orden; los omitidos toman su valor por defecto
  ```

  - **Un parámetro sin expresión por defecto tiene el tipo `Option<Type>`.** Si se omite, es `none`; si se
    pasa, el valor simple que escribió quien llama se envuelve automáticamente en `some`. Lo que CL hace con
    una variable supplied-p ("¿se proporcionó?") aparece aquí del lado del tipo estático.
  - Con una expresión por defecto, el tipo sigue siendo `Type` tal como se declaró. Si se omite, esa
    **expresión comprobada** se incrusta tal cual en la llamada (se evalúa en cada llamada).
  - **`&key` no se puede mezclar con `&optional`/`&rest` en una misma lista de argumentos.** Así se evita una
    ambigüedad que tiene el propio CL (si un argumento real final lo toma un `&optional` posicional o se
    empareja por etiqueta como `&key` depende de los *valores*) prohibiendo la combinación. `&optional` y
    `&rest` se pueden usar juntos.
  - Se pueden usar en funciones genéricas, pero **un parámetro de tipo que solo aparece en argumentos
    omitidos no se puede inferir y es un error** (no hay valor con el que emparejarlo).
  - **Los métodos de `impl` pueden tener las mismas tres secciones** (tanto los métodos de instancia
    como los estáticos). Un método de instancia pone `&optional`/`&rest`/`&key` después de `self`:

    ```lisp
    (defstruct box (w i32) (h i32))
    (impl box
      (grow (self &key (dw i32 0) (dh i32 0)) i32 ...))
    (grow (make-instance 'box :w 1 :h 2) :dh 10)

    (defstruct point (x i32) (y i32))
    (impl point
      (origin (&key (x i32 0) (y i32 0)) point (make-instance 'point :x x :y y)))   ; método estático
    (point::origin :y 7)
    ```

    También se pueden usar en métodos de tipos genéricos, pero **el tipo de un parámetro con expresión por
    defecto no puede mencionar los parámetros de tipo del propietario** (la misma restricción que tiene
    `defun` para sus propios parámetros de tipo: lo que se incrusta cuando se omite el argumento es una
    expresión *comprobada*, así que su tipo no se puede dejar como una variable abstracta).
  - **No se pueden usar en métodos de trait.** Esto incluye los métodos escritos en el `impl` de un trait. `deftrait` no tiene sintaxis para ellas, y si solo el lado del
    `impl` pudiera declarar secciones, las llamadas con un receptor `:dyn` (que rellenan los argumentos a partir
    de la declaración del trait) y las llamadas con un receptor concreto (que los rellenan a partir de la
    declaración del `impl`) se convertirían en cosas distintas. La aridad de una ranura de vtable es fija.
  - **No se pueden usar en `lambda` / `labels`** (`&rest` sí). Para rellenar un argumento omitido, quien llama
    tiene que leer **la expresión por defecto comprobada de quien es llamado**, que solo está disponible a
    partir de una firma resuelta por nombre. Una `lambda` se pasa como valor, y lo único que describe ese
    valor es su tipo de función `(fn ...)`: en él no hay sitio para una expresión, y si lo hubiera, "dos
    lambdas con la misma firma pero distintos valores por defecto" se convertirían en tipos distintos. `&rest`
    se queda en el terreno de los tipos, así que se puede escribir en un tipo de función.
- **Las referencias anticipadas se declaran con `defsignature`** (abajo). Un nombre que no se ha declarado no
  se puede llamar antes de su definición, porque el nivel superior se comprueba y se ejecuta una forma cada
  vez, en el orden del código fuente.
- Para exigir restricciones de trait, escribe una cláusula `:where` justo antes del cuerpo:
  `(defun name<T> (params) Ret (:where (Trait T (AssocName ConcreteType)...)) body...)`
  (fijar un tipo asociado con `(AssocName ConcreteType)` es opcional).
- **Docstrings**: un literal de cadena al principio del cuerpo, justo después de la cláusula `:where` (si la
  hay), se convierte en la docstring (como en CL). Pero solo cuando lo sigue al menos una forma del cuerpo:
  una cadena sola sigue siendo el valor de retorno y no se toma como docstring:
  `(defun f () string "doc" "value")` tiene docstring y devuelve `"value"`, mientras que
  `(defun f () string "value")` no tiene docstring y devuelve `"value"`. Se puede recuperar con
  `(documentation name)` ([docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — declaraciones anticipadas

```lisp
(defsignature name (tipos-de-argumentos...) tipo-de-retorno)
(pub defsignature name (tipos-de-argumentos...) tipo-de-retorno)
```

Para llamar a un `defun` definido **después** de ti, declárala primero así. La recursión mutua solo se puede
escribir de esta forma:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Los argumentos se enumeran **solo por sus tipos**; no hay cuerpo, así que no hay nada a lo que dar nombre.
`&rest` se puede escribir al final, como `&rest tipo-de-elemento`.

Las declaraciones **se comprueban**:

- La definición que sigue debe coincidir con la declaración (el número y los tipos de los argumentos, el tipo
  de retorno, `&rest` y si es `pub`). Una discrepancia es un error en la definición.
- Declarar sin definir es un error (se informa cuando termina de cargarse el archivo / módulo). El REPL no lo
  informa tras cada entrada, porque una declaración y su definición deben poder teclearse en líneas
  distintas.
- Una declaración colocada **después** de la definición es un error, ya que no podría hacer nada.

Hay tres cosas que no se pueden declarar:

- **Funciones genéricas.** Hacer una copia para cada tipo necesita el cuerpo, y una declaración no lo tiene.
  Una llamada anticipada se podría resolver pero la instanciación fallaría, así que la declaración se rechaza
  de entrada.
- **`&optional`/`&key`.** Su firma incluye la expresión **comprobada** de cada valor por defecto (incrustada
  en la llamada cuando se omite el argumento), y una declaración no tiene sitio para ella.
- **Cualquier cosa que no sea `defun`.** Un `defmacro` necesita que el cuerpo de la macro **ya se haya
  ejecutado** para poder expandir, algo que registrar una firma no puede sustituir. Para los tipos
  (`defstruct`/`defenum`/`deftrait`), registrarlos es "lo que necesita el propio código que registra el tipo",
  que no es autocontenido como lo es una firma. Los métodos de un `impl` se registran en el tipo que los posee, así que siguen al tipo.

El equivalente en CL es `(declaim (ftype (function (i32) bool) even2))`, pero eso viene con todo un sistema de
declaraciones y es solo **orientativo**. Aquí, con tipado estático, las declaraciones se comprueban.

### 3.3 defffi — declarar funciones de C (FFI)

```lisp
(defffi (nombre "c_symbol") (tipos-de-argumentos...) tipo-de-retorno)
(defffi (nombre "c_symbol") (tipos-de-argumentos...) tipo-de-retorno :library "nombre")
(defffi nombre (tipos-de-argumentos...) tipo-de-retorno)              ; nombre = el nombre del símbolo en C
(pub defffi ...)
```

Declara una función de C para poder llamarla. La forma es la misma que `defsignature` (un nombre, tipos de
argumentos, un tipo de retorno y sin cuerpo), pero no tener cuerpo significa otra cosa. `defsignature` es una
promesa de que "la definiré más tarde", mientras que `defffi` declara que "alguien más ya ha escrito y
compilado el cuerpo".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

El nombre en typelisp y el nombre del símbolo en C se pueden escribir por separado porque los identificadores
de typelisp suelen contener `-` y los de C no pueden. Si se omite el nombre de C, el nombre se usa tal cual
como nombre del símbolo en C.

**Las llamadas requieren `(unsafe ...)`** (incluso para funciones que solo usan escalares). El compilador no
tiene forma de confirmar que la firma de C declarada coincide con la real y solo puede fiarse de la
declaración; `unsafe` es la marca de que asumes esa responsabilidad. La forma prevista es envolverla una vez
y crear un envoltorio seguro:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; a partir de aquí no hace falta unsafe
```

Los tipos que se pueden escribir son `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, y los punteros tipados `(ptr T)`
([abajo](#def-c-struct-y-punteros-tipados--reservar-structs-de-c)).

`string` es `const char *`. Las cadenas de typelisp no terminan en NUL y pueden contener NUL, así que **se
copian en una cadena de C al pasarse**, y se liberan tras la llamada. Un NUL en la cadena es un error: C solo
miraría hasta él, así que se pasaría en silencio una cadena distinta.

**Las cadenas devueltas también se copian**, y no se liberan: lo que devuelve C pertenece a C, y puede apuntar
a una tabla estática, como con `getenv`. Las funciones que devuelven memoria que quien llama debe liberar
(`strdup`, etc.) se deben recibir como `ptr` y liberar por tu cuenta.

Las funciones cuyo resultado apunta dentro de un argumento (`strchr`, `strstr`) también funcionan
correctamente: el resultado se copia antes de liberar el argumento.

Si una función declarada para devolver `string` devuelve NULL, es un error, porque `string` no tiene ningún
valor que signifique "no había nada". Si NULL es posible, recibe el resultado como `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Con `:library`, se abre esa biblioteca compartida y se busca el símbolo en ella. Sin ella, el símbolo se busca
en **el propio proceso** (todo lo ya enlazado, incluida libc). Un nombre corto como `sqlite3` se busca como
`libsqlite3.dylib` / `libsqlite3.so` en ese orden, y un nombre que contiene `/` se trata como una ruta. Las
bibliotecas abiertas nunca se cierran: el código que apunta a sus funciones sigue ejecutándose, así que la
única vida correcta es la del proceso.

#### ptr / c-long / c-ulong — palabras de máquina en bruto

`ptr` es un puntero opaco (`void *`, `FILE *`, lo que sea que quisiera decir la declaración). `c-long` /
`c-ulong` son el `long` / `unsigned long` de C (también `size_t`, `int64_t` e `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**No llamarlos `i64` / `u64` es deliberado.** Este lenguaje no tiene tipo entero de 64 bits, porque un
inmediato etiquetado solo tiene 63 bits ([capítulo 2](#2-escritura-de-tipos)). El nombre `c-long` dice "esto
es una palabra que cruza la frontera con C, no un entero de este lenguaje".

**No tienen aritmética.** No se puede escribir `(+ x 1)`. Se podría ofrecer pero no se ofrece, para que no se
haga ningún cálculo sobre un valor que no se puede almacenar en ningún sitio y que tiene un ancho distinto de
todos los demás números, por el mismo motivo por el que se dejó fuera el tipo entero de 64 bits. Solo hay
**conversiones**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; leer lo que se devolvió
(as int (unsafe (c-strlen s)))               ; esta para leerlo exactamente (int no pierde 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; preguntar si cabe
(as c-ulong n)                               ; crear uno a partir de otro entero
```

Los **literales** enteros toman el tipo esperado, así que no hace falta `as` solo para pasar uno:

```lisp
(unsafe (c-malloc 16))                       ; 16 se lee como un c-ulong
```

Los literales fuera de rango se rechazan como con los demás anchos (`(c-malloc -1)` no cabe en un `c-ulong`).

**Los lugares donde pueden aparecer están limitados**: solo tipos de argumento, tipos de retorno y variables
locales. Cada uno de los siguientes es un error:

```lisp
(defstruct handle (p ptr))          ; un campo de estructura
(defenum maybe (none) (some ptr))   ; un campo de enumeración
(defvar (block ptr) ...)            ; una global
(defffi f ((vector ptr)) i32)       ; dentro de un argumento de tipo
```

Hay un único motivo para todos: **la ranura etiqueta lo que contiene**. Etiquetar haría perder los bits
superiores del puntero, el mismo motivo por el que se dejó fuera el tipo entero de 64 bits, así que no se
permite ni siquiera en `unsafe`. No es una cuestión de permiso: esa representación no existe.

Por el mismo motivo, no pueden ser variables locales **capturadas** por funciones anidadas (un enlace
capturado va a una celda, y una celda etiqueta lo que contiene). Esto se sabe en tiempo de compilación y lo
informa `(compile f)`.

El GC no rastrea `ptr`. Apunta fuera del montículo, así que es lo correcto.

Hay cuatro cosas que no se pueden declarar:

- **Argumentos variádicos** (`printf`). La parte variádica se pasa con reglas distintas de las de los
  argumentos fijos (en la pila en AArch64 Darwin), así que no se puede llamar correctamente desde una firma
  fija. `&rest` se rechaza.
- **Pasar o devolver structs por valor.** Por el mismo motivo (depende de la convención de llamada de cada
  plataforma). Los tipos que se pueden escribir se limitan a la lista de arriba, así que no se puede escribir.
- **Genéricos.** C no tiene equivalente.
- **El mismo nombre que una función incorporada.** Una llamada compilada resolvería ese nombre a la función
  incorporada, así que se rechaza en lugar de fallar en silencio.

#### Callbacks — que C llame de vuelta

Escribir un tipo de función `(fn (tipos...) tipo-de-retorno)` como tipo de un argumento hace de ese argumento
una función a la que C llama de vuelta (un callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; una función de nivel superior
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; una lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; una función local
```

Un puntero a función de C no es más que una dirección de código, y C la llama pasando solo los argumentos
declarados. No hay sitio para pasar variables capturadas, así que **solo se pueden pasar funciones sin
variables libres**, y esto se comprueba al comprobar los tipos.

- Escribe un nombre de función o una expresión `lambda` **directamente** como argumento real. No se puede
  pasar una variable que contiene una función: qué función contiene, y por tanto si tiene variables libres,
  no se sabe hasta el tiempo de ejecución.
- Una `lambda` es un error si se refiere a variables locales de fuera de ella. Se puede referir a variables
  globales y a funciones de nivel superior.
- Una función local (`labels`) no debe tener variables libres, incluidas las de las funciones hermanas a las
  que llama. Las funciones hermanas comparten el lugar donde se guardan las variables capturadas, así que lo
  que captura una hermana llamada también lo captura esta función.
- Una función genérica obtiene sus tipos del tipo de función declarado.
- Los tipos que se pueden escribir en el tipo de función son los mismos que los de la lista de arriba. Sin
  embargo, `string` no puede ser el tipo de retorno de un callback (le entregaría a C memoria que nadie
  libera). Un argumento `string` copia la cadena que pasó C en una cadena de typelisp.

Las llamadas a funciones de C solo se pueden escribir dentro de `unsafe`, así que los callbacks solo se pueden
pasar dentro de `unsafe`.

**El callback solo se puede llamar mientras se ejecuta la función de C a la que llamó typelisp.** Si se llama
desde cualquier otro sitio (un hilo que no ejecuta typelisp, un manejador de señales, una función registrada
con `atexit`), imprime el motivo y detiene el proceso.

**Los fallos no se propagan a través de C.** Un `panic` o `throw` dentro del callback no puede desenrollar a
través de los marcos de C (sería comportamiento indefinido), así que se devuelve 0 a C, y el fallo se vuelve a
lanzar a quien llamó cuando retorna la función de C. Si el callback se vuelve a llamar entre el fallo y el
retorno de la función de C, no se ejecuta y se devuelve 0.

Una operación que tendría que esperar dentro de un callback (un `recv` sobre un canal vacío, etc.) es un
error ([12.6](#126-código-compilado-y-tareas)).

Cuando se redefine una función, la nueva definición se llama a partir de la siguiente vez que se pasa a C.

Funciona igual con AOT (`compile-file`). Los puntos de entrada a los que llama C se incorporan al ejecutable.

**No se pueden pasar como valores.** Una declaración FFI no se puede escribir tal cual como el `f` de
`(map f xs)`: un valor de función es un cierre que envuelve el cuerpo de una definición, y esta declaración no
tiene cuerpo que envolver. Envuélvela en una `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` también se rechaza: lo que se podría mostrar es el código máquina de C, que este
compilador no produjo. `(compile c-abs)` tiene éxito (y no hace nada, ya que ya está compilado).

**También funciona con AOT (`compile-file`).** El enlazador resuelve las propias funciones de C. Si una
declaración tiene `:library`, esa biblioteca se añade a la línea de enlazado como `-l` (los duplicados se
reúnen en uno), así que `compile-file` no necesita argumentos extra. El propio `compile-file` lee el código
fuente, así que puede recogerlas de las declaraciones.

Los símbolos también se buscan al compilar. Si una función declarada no existe, el error la nombra antes de
cualquier error de enlazado.

La biblioteca estándar (el prelude) no usa `defffi`. La biblioteca estándar entra entera en cada ejecutable,
así que una declaración con `:library` en ella enlazaría esa biblioteca incluso en programas que no usan el
FFI.

#### def-c-struct y punteros tipados — reservar structs de C

```lisp
(unsafe
  (def-c-struct nombre (campo tipo)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Declara un struct con la misma disposición que en C. Solo se puede escribir dentro de un `unsafe` de nivel
superior (que no puede contener nada más que `def-c-struct`). Se puede poner una docstring justo después del
nombre.

Los tipos que se pueden escribir para los campos son `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong`
`f32` `f64` `bool` `ptr`, los punteros tipados `(ptr T)` y otros `def-c-struct` (incrustados por valor). La
disposición (el desplazamiento de cada campo, y el tamaño y la alineación del struct) se calcula con las
reglas de C (suponiendo LP64). Se puede escribir un campo que apunta al propio struct, pero el struct no se
puede incrustar a sí mismo.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x en 0, y en 8, tamaño 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

El nombre de un `def-c-struct` entra en el espacio de nombres de tipos (no puede haber un `defstruct` o
similar con el mismo nombre en el mismo módulo), pero **no es el tipo de un valor**. No puedes escribir
`(defun f ((p point)) ...)`; solo aparece como aquello a lo que apunta un puntero tipado.

**Un puntero tipado `(ptr T)`** es una dirección que apunta a un `T`. `T` es uno de los tipos que se pueden
escribir para los campos de arriba. Es una palabra de máquina en bruto como `ptr`, con las mismas reglas sobre
dónde puede aparecer (solo argumentos, tipos de retorno y variables locales; solo puede ser un valor dentro de
`unsafe`).

La reserva, la lectura y la escritura se escriben con las siguientes formas. Todas solo se pueden usar dentro
de `unsafe`.

| Forma | Significado |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Reserva `n` valores de `T` (1 si se omite). El contenido se rellena con 0. Devuelve un `(ptr T)` |
| `(c-ref p i)` | Un puntero al elemento `i` desde `p`. Error si está fuera del rango reservado |
| `(c-deref p)` / `(setf (c-deref p) v)` | Lee / escribe el escalar al que apunta `p` |
| `p::field` / `(setf p::field v)` | Lee / escribe un campo de un struct. Leer un campo que es un struct incrustado da su dirección (`(ptr tipo-interior)`) |
| `(as ptr p)` | Olvida el tipo, convirtiéndolo en un `ptr` (para pasarlo a algo como el `void *` de `qsort`). No hay conversión de vuelta |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**La memoria reservada se libera cuando el control sale del `unsafe` que la reservó.** El propietario es el
`unsafe` léxicamente más externo dentro de la misma función. Se libera tanto si el código termina normalmente
como si sale por `panic`, `throw` o `return-from`. Las funciones de `lambda` y `labels` son funciones aparte,
así que un `c-alloc` en ellas necesita un `unsafe` propio dentro de ellas.

Por eso, un puntero tipado no puede salir del `unsafe` que lo reservó. Cada uno de los siguientes es un error
de tipos:

- Hacerlo el valor de la expresión `unsafe` (así que tampoco se puede devolver de una función)
- Capturarlo en un cierre (`lambda`, `labels`)
- Pasarlo a `task` / `thread`
- Lanzarlo con `throw`

Para usar valores fuera del `unsafe`, cópialos en un `defstruct` o en números dentro del `unsafe` y devuelve
eso.

**La memoria reservada del lado de C no se trata.** Los valores que llegan de C como punteros tipados (valores
de retorno de `defffi`, argumentos de callbacks, valores leídos de campos de tipo puntero) se comprueban en
tiempo de ejecución para ver si apuntan a un valor de ese tipo dentro de una reserva viva de `c-alloc`, y son
un error si no. NULL también es un error. Para recibir memoria que reservó C, o NULL, usa el `ptr` sin tipo
(cuyo contenido no se puede leer).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Cuando la comprobación rechaza el argumento de un callback, se informa a quien llamó cuando retorna la
función de C, igual que un fallo dentro de un callback.

### 3.4 defvar / defparameter / defconstant — variables globales

```lisp
(defvar (name Type) init-expr)        ; inicializa solo si aún no está ligada
(defparameter (name Type) init-expr)  ; asigna cada vez
(defconstant (name Type) init-expr)

; con docstring (en el mismo orden que defvar/defparameter/defconstant de CL: después del valor)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**La diferencia entre `defvar` y `defparameter` se ve al recargar** (como en CL). Si la global **ya está
ligada, `defvar` ni siquiera evalúa el inicializador**, así que cuando editas un archivo de configuración y lo
vuelves a leer, los valores que cambió la sesión se quedan como están. `defparameter` asigna cada vez, así que
volver a leerlo devuelve los valores a lo que está escrito.

La anotación de tipo es obligatoria (no se infiere del inicializador). `defvar` se puede cambiar;
`defconstant` no (`setf` es un error).

### 3.5 impl — métodos de un tipo

```lisp
(impl Type
  (name ((arg Type2) ...) RetType body...)            ; método estático: (Type::name args...)
  (name (self (arg Type2) ...) RetType body...)       ; método de instancia: (name obj args...)
  (pub name ...))                                      ; público también fuera del módulo del tipo

; un tipo genérico
(impl Type<T> (:where (Trait T)...) item...)
```

Da métodos a un tipo definido con `defstruct` (3.6) o `defenum` (3.7). Corresponde al impl inherente
de Rust (`impl Point { ... }`).

- Un elemento tiene la forma `(nombre (parámetros...) TipoDeRetorno cuerpo...)`, igual que un elemento
  del `impl` de un trait (3.9).
- Un elemento cuyo primer parámetro es `self` es un **método de instancia**, que se llama como
  `(name obj args...)`. `self` no necesita tipo (`(self)` es lo mismo que `((self Self))`, y `Self` es el
  tipo objetivo).
- Un elemento sin `self` es un **método estático**, que se llama como `(Type::name args...)`. Un método
  estático que crea un valor es el constructor de ese tipo. El nombre es libre; se recomienda `new`.
- **La visibilidad es por elemento.** Solo los elementos escritos `(pub name ...)` se pueden usar fuera
  del módulo que define el tipo. Sin `pub`, un elemento solo se puede usar dentro de ese módulo.
- Un tipo puede tener cualquier número de `impl`.
- Quien llama resuelve el método por el tipo estático de `obj` (despacho único y estático). Un método se
  busca **solo por el tipo del primer argumento** (3.10).
- Los **docstrings** se pueden poner en la misma posición y con las mismas reglas que en `defun` (al
  principio del cuerpo, solo cuando lo siguen formas del cuerpo). Se obtienen con
  `(documentation Type::name)`. Lo mismo vale para los métodos del `impl` de un trait.
- Solo los tipos del propio programa pueden tenerlo. Un tipo de la biblioteca estándar como `Vector`,
  `Option` o `int` no puede (el error dice `... is a standard library type`). Para añadir una operación a
  un tipo estándar, define un trait y haz su `impl` para ese tipo.
- En `(impl X ...)`, si lo que sigue a `X` es un elemento (una lista), un `:where` o nada, es este impl
  inherente; si es un nombre, es el `impl` de un trait (`(impl Trait Type ...)`, 3.9).

```lisp
(defstruct Point (pub x f64) (pub y f64))

(impl Point
  (pub new ((x f64) (y f64)) Point
    (make-instance 'Point :x x :y y))
  (pub origin () Point
    (make-instance 'Point :x 0.0 :y 0.0))
  (pub area (self) f64
    (* self::x self::y)))

(Point::new 1.0 2.0)          ; método estático
(area (Point::new 2.0 3.0))   ; método de instancia. => 6.0
```

Los parámetros de tipo propios de un método se escriben en su nombre con `<...>`, como en `defun`. Los
parámetros de tipo del tipo objetivo (`T` abajo) los fija el receptor; los propios del método (`U`) se
infieren de los argumentos de cada llamada.

```lisp
(defstruct Box<T> (v T))

(impl Box<T>
  (fmap<U> (self (f (fn (T) U))) Box<U>
    (make-instance 'Box :v (f self::v))))

(fmap (make-instance 'Box :v 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Da a los parámetros de tipo propios del método nombres distintos de los parámetros de tipo del tipo
  objetivo escritos en el `impl` (la `T` de `(impl Box<T> ...)`).
- Un método del `impl` de un trait no puede añadir parámetros de tipo: su firma sigue la que declara el
  trait.

El `impl` de un tipo genérico escribe todos sus parámetros de tipo como variables (`Box<T>`, común a todas
las instancias) o todos como tipos concretos (`Box<int>`, solo para esa instancia). Los `impl` de instancias
distintas son implementaciones separadas, así que `Box<int>` y `Box<string>` pueden tener cada uno un método
con el mismo nombre.

```lisp
(defstruct Box<T> (v T))

(impl Box<int>
  (twice (self) int (* 2 self::v))
  (make () Box<int> (make-instance 'Box :v 7)))
(impl Box<string>
  (twice (self) string (format false "~a~a" self::v self::v))
  (make () Box<string> (make-instance 'Box :v "s")))

(twice (make-instance 'Box :v 4))      ; => 8
(twice (make-instance 'Box :v "ab"))   ; => "abab"
(twice (the Box<int> (Box::make)))     ; => 14
```

- Un método de instancia se elige por el tipo del receptor. Una instancia sin `impl` (por ejemplo,
  `Box<f64>`) no tiene ese método.
- Un método estático se elige por el tipo que se espera que devuelva la llamada (el `(the Box<int> ...)`
  de arriba). Si solo una instancia tiene un método estático con ese nombre, no hace falta tipo esperado.
- Un método con un nombre no puede escribirse a la vez en un `impl` con variables (`Box<T>`) y en el `impl`
  de una instancia (`Box<int>`); en cualquier orden es un error. El accesor de un campo cuenta como un
  método del `impl` con variables.
- Para nombrar uno de esos métodos en `documentation`, `compile`, `trace` y similares, escribe la instancia:
  `Box<int>::twice`. Si solo una instancia tiene el método, también vale `Box::twice`.

### 3.6 defstruct — estructuras (tipos definidos por el usuario)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; genérica (parámetros de tipo entre corchetes angulares)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Cada campo es `(name type)` o `(pub name type)` (visibilidad por campo, independiente del `pub` de la propia
  estructura). Una expresión más al final se convierte en el **valor por defecto** de la ranura (`(x i32 0)`).
- Los valores se crean con `make-instance` (abajo). No se genera ningún constructor que tome los campos en
  orden. Si quieres un constructor, escríbelo en `impl` (3.5) como método estático.
- Se genera automáticamente lo siguiente:
  - Los lectores `(field-name instance)`, con la escritura `instance::field-name`
  - Los escritores `(set-field-name instance value)`, con la escritura `(setf instance::field-name value)`
- Para hacer `pub` la propia estructura, pon `pub` delante, como en `(pub defstruct ...)`.
- **Define un tipo antes de nombrarlo.** El tipo de un campo puede ser la propia estructura
  (`(next Option<node>)`), pero no un tipo definido después: los tipos no tienen una declaración anticipada
  que corresponda a `defsignature`. Un nombre aún no definido da el mismo error `unknown type` en un tipo de
  argumento de `defun` o en `the`. Así que no se pueden escribir dos tipos que se refieran el uno al otro.
- **Las variables de tipo son solo las escritas en posiciones de declaración.** Para
  `defun`/`defstruct`/`defenum`/`deftype`, el `<T>` del nombre; para un `impl` inherente, el tipo objetivo
  (`(impl box<T> ...)`) y el `<U>` del nombre de cada método; para el `impl` de un trait, el tipo objetivo
  e `impl<T>`; para `deftrait`, `Self` y los tipos asociados de `(:type Item)`. Un nombre que aparece por
  primera vez en cualquier otro sitio (argumentos, valor de retorno, `the`/`lambda` en el cuerpo) no se
  convierte en variable de tipo; es `unknown type`.
- **Docstrings**: un literal de cadena justo después del nombre, antes de los campos, se convierte en la
  docstring (`(defstruct Name "doc" (field Type)...)`, la misma posición que en el `defstruct` de CL). Un campo
  siempre tiene la forma `(name Type ...)` y nunca puede ser una cadena suelta, así que no hay ambigüedad. Se
  recupera con `(documentation Name)`.

#### make-instance — crear un valor

```lisp
(make-instance 'Name :field1 value1 :field2 value2 ...)
```

Un valor de estructura se crea con `make-instance`, como en CLOS.

```lisp
(defstruct point (pub x i32) (pub y i32 0))

(make-instance 'point :x 1 :y 2)
(make-instance 'point :y 2 :x 1)      ; en cualquier orden
(make-instance 'point :x 1)           ; y toma su valor por defecto, 0
```

- El nombre del tipo se escribe como un símbolo citado con `'`. No elige un tipo en tiempo de ejecución;
  queda fijado en el momento de la comprobación (como el `'vector` de `concatenate`).
- Los campos se dan como pares `:nombre valor`. Los valores se evalúan una vez cada uno, en el orden
  escrito.
- Un campo no dado toma su valor por defecto. No dar un campo sin valor por defecto es un error de tipo, y
  también lo son un campo desconocido y el mismo campo dado dos veces.
- Los argumentos de tipo de una estructura genérica se infieren de los valores y del tipo esperado.
  También se pueden escribir, como en `'Box<int>`.

  ```lisp
  (defstruct Box<T> (v T))
  (make-instance 'Box :v 1)                    ; Box<int>
  (the Box<f64> (make-instance 'Box :v 1.5))   ; Box<f64>
  (make-instance 'Box<int> :v 7)
  ```

- Fuera del módulo que define el tipo no se pueden dar campos que no sean `pub`. Si un campo así no tiene
  valor por defecto, los demás módulos no pueden crear el valor. Para permitirlo, escribe un constructor
  `pub` en `impl` (3.5).

#### Lista de opciones

Escribir una lista `(Name option...)` en la posición del nombre especifica opciones (la misma posición que
en CL).

```lisp
(defstruct (point (:copier copy-point))
  (x i32 0)          ; un tercer elemento es el valor por defecto de esa ranura
  (y i32 0))

(copy-point p)       ; una copia superficial (igual que el copier de CL)
```

- **`:copier`**: genera un **método de instancia** que devuelve un valor nuevo con los mismos valores de
  ranura. Superficial, como el copier de CL.
- **`:include Parent`**: antepone las ranuras del padre (también se heredan los valores por defecto; el padre
  puede estar en otro archivo). **No crea ninguna relación de tipos**: el hijo no es un subtipo del padre, los
  métodos del padre no se aplican al hijo, y no hay ninguna comprobación en tiempo de ejecución que los una.
  Este lenguaje no tiene subtipado; las interfaces comunes son cosa de `deftrait`. Solo se une la *lista* de
  ranuras.
- Opciones que se dejan fuera, y por qué:
  - **`:constructor`**: un constructor se escribe en `impl` como método estático (3.5). Escribir esta
    opción es un error que indica cómo reescribirla.
  - **`:conc-name`**: en CL antepone un prefijo a los accesores para evitar choques en un único espacio de
    nombres de funciones plano. Aquí, los accesores son métodos despachados según el tipo del receptor, así
    que no se producen choques, y un prefijo rompería `instance::field` (que solo conoce el nombre de la
    ranura).
  - **`:predicate`**: responde en tiempo de ejecución "¿es este valor un `point`?". Aquí los tipos son una
    clasificación en tiempo de compilación sin testigo en tiempo de ejecución, y no hay ninguna posición donde
    exista "un valor de tipo desconocido que podría ser un point" (`match` sobre `Sexpr` está cerrado, y `:dyn`
    no admite conversión descendente), así que un predicado generado solo podría devolver siempre `true`.
  - **`:type` / `:initial-offset` / `:named`**: sustituyen la representación del valor por una lista o un
    vector. La representación pertenece al compilador y no se puede observar desde el lenguaje.

### 3.7 defenum — enumeraciones (tipos suma)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; una variante con carga útil (campos posicionales)
  (Variant2)                  ; una variante sin carga útil
  ...)

; genérica
(defenum Option<T>
  (some T)
  (none))
```

- Cada variante tiene la forma `(VariantName FieldType...)`. Los campos son solo posicionales (no tienen
  nombre). Hace falta al menos una variante, y los nombres no se pueden repetir.
- Los valores se construyen, como con los `Option`/`Result` incorporados, calificados o mediante `use`:
  `(Name::Variant1 a b)`, o `(Variant1 a b)` tras `(use Name)`.
- Se pueden desarmar con `match` / `if-let`. `match` comprueba la exhaustividad (debe cubrir todas las
  variantes o tener un `_`):
  ```lisp
  (match opt
    ((some v) v)
    ((none) 0))
  ```
- Los métodos y los métodos estáticos se escriben en `impl` (3.5), como en `defstruct`.
- Para hacer `pub` la propia enumeración, escribe `(pub defenum ...)`.
- **Docstrings**: la misma posición y las mismas reglas que `defstruct`, justo después del nombre, antes de las
  variantes (`(defenum Name "doc" (Variant ...)...)`). Se recupera con `(documentation Name)`.

### 3.8 deftype — alias de tipo

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

El `deftype` de CL, reducido a lo que tiene sentido en un lenguaje de tipado estático: **una forma de escribir
un tipo, no un tipo**.

- La posición del nombre es la misma que en `defun`, y los argumentos genéricos se escriben `Name<T,U>`. En el
  lugar de uso, hace falta exactamente el número declarado de argumentos de tipo (demasiados o demasiado pocos
  es un error en el acto).
- La expansión ocurre **dentro del analizador de tipos**. Así que nada aguas abajo sabe que existe el alias:
  las claves de monomorfización, los volcados, el camino de compilación y **los mensajes de error** muestran
  todos la forma expandida. Si `(f "x")` falla contra una función que exige `meters`, el mensaje dice `i32`.
- **No es un tipo nuevo.** `(deftype meters i32)` hace que `meters` e `i32` sean el mismo tipo, así que
  mezclarlos no se detecta. Si quieres mantenerlos separados, usa `defstruct`.
- **No es un predicado.** El `(deftype small () '(integer 0 9))` de CL describe un *conjunto de valores* que
  `typep` comprueba en tiempo de ejecución, pero aquí los tipos son una clasificación en tiempo de compilación
  sin testigo en tiempo de ejecución, así que un alias que restringiera valores no tendría nada que
  restringir.
- **No se puede contener a sí mismo.** Un alias se expande donde se escribe, así que no tiene adónde recursar.
  Los tipos de datos recursivos se escriben con `defstruct`/`defenum`.
- Comparte el espacio de nombres con los tipos y los traits (dentro de un módulo no puede tener el mismo nombre
  que un `defstruct`/`defenum`/`deftrait`). Se hace público con `(pub deftype ...)` y se trae con
  `(use m::meters)`.
- **Docstrings**: justo después del nombre, antes del tipo (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; la lista de supertraits es obligatoria; () si no hay ninguno
  (:type AssocName)                       ; tipos asociados (cualquier número, opcionales)
  (method-name ((self Self) params...) RetType)          ; sin cuerpo = hay que implementarlo
  (method-name ((self Self) params...) RetType body...)) ; con cuerpo = implementación por defecto

(impl TraitName TargetType
  (:where (Trait A)...)                   ; restricciones que se aplican a todo el impl (opcional)
  (:type AssocName ConcreteType)          ; concreta un tipo asociado
  (method-name (recv params...) RetType body...))
```

Mediante `impl`, cada método se registra como método de `TargetType` y se llama como un método de un `impl`
inherente (3.5). Los métodos del `impl` de un trait son siempre públicos, sin `pub`. Los traits se usan como
cotas de trait en las cláusulas `:where` de las funciones genéricas (ver [3.1 defun](#31-defun--definición-de-funciones)). El nombre de un trait
también puede ser una ruta `::` como `m::Trait`.

**La lista de supertraits (obligatoria)**: siempre se escribe justo después del nombre del trait. Cada
elemento es un nombre de trait simple o, si ese trait tiene tipos asociados, `(Trait (Assoc Type))` con
**todos sus tipos asociados fijados**.

```lisp
(deftrait Eq () ...)                       ; sin supertraits
(deftrait Ord (Eq) ...)                    ; el trait Ord: Eq de Rust
(deftrait CharSource ((Iter (Item char)))  ; fijar un tipo asociado
  (rewind ((self Self)) ()))
```

La herencia tiene tres efectos. (1) `impl Ord X` exige que `impl Eq X` se escriba **antes** (una regla sobre el
orden de escritura: la única forma que se puede decidir de manera determinista en el REPL y con `load` paso a
paso, y más estricta que Rust). (2) `(:where (Ord T))` basta para poder llamar también a los métodos de `Eq`.
(3) Los métodos de `Eq` se pueden llamar a través de un `:dyn Ord`, y un valor `:dyn Ord` se puede pasar tal
cual donde se exige un `:dyn Eq` (conversión ascendente). Que un subtrait vuelva a declarar un método con el
mismo nombre que su padre, y heredar métodos con el mismo nombre de dos padres, son ambos errores (una vtable
tiene una ranura por nombre). La herencia en diamante se funde en una sola ranura.

**Implementaciones por defecto**: un cuerpo tras la firma se usa cuando un `impl` omite el método. El cuerpo se
resuelve en **el espacio de nombres del módulo** donde está escrito el trait, así que puede llamar a funciones
no públicas de ese módulo. Los métodos con cuerpo también pueden tener cláusulas `:where` y docstrings. El
cuerpo se comprueba **una vez, en el punto de declaración**, con `Self` como variable de tipo (restringida por
`Self: el propio trait`), como en Rust: los errores que fallarían para todo `impl` y todo tipo que lo
implemente, incluso en implementaciones por defecto que ningún `impl` omite nunca, se detectan ahí. Las
llamadas sobre `self` a métodos del propio trait o de sus supertraits pasan por esta restricción, y los tipos
asociados se fijan a sí mismos, así que una firma que devuelve `Item` se contrasta con el cuerpo sin conocer
el tipo concreto.

**Implementaciones por instancia**: un tipo genérico puede tener un `impl` distinto para cada instancia
(`(impl Show Box<int> ...)` y `(impl Show Box<string> ...)`). `:dyn`, las restricciones `:where`, los tipos
asociados y `print-object` funcionan por instancia, y una instancia sin `impl` (`Box<f64>`) no implementa el
trait. No se puede implementar el mismo trait para `Box<T>` y para `Box<int>` a la vez (la misma regla que
para los métodos de 3.5). Basta con que el supertrait esté implementado en esa instancia (por un `impl` de
`Box<int>` o de `Box<T>`).

**Implementaciones generales (blanket)**: hacer que el destino sea una variable de tipo implementa el trait de
una vez para todo tipo que cumpla las restricciones.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (:where (Ord T)))          ; sin cuerpo; todo es la implementación por defecto
```

**No se genera código hasta que un tipo concreto lo usa realmente** (una vez por tipo, con el mismo mecanismo
que la monomorfización normal). Un trait puede tener como mucho una implementación general. Si un tipo tiene
un `impl` explícito, ese tiene prioridad. La comprobación de tipos del cuerpo es independiente de la
generación: se hace una vez en el punto de declaración, **con el destino como variable de tipo** (como en
Rust), así que incluso una implementación que nunca se usa tiene sus errores detectados ahí si fallarían para
todo destino bajo las restricciones declaradas. Las llamadas justificadas por las restricciones
(`(less self other)` bajo `(:where (Ord T))`, etc.) pasan, como en el cuerpo de un `defun` genérico.

**Docstrings**: un `deftrait` puede tener una docstring para todo el trait, como literal de cadena justo
después de la lista de supertraits, antes de los elementos (`(deftrait Name () "doc" (:type ...) (method ...)...)`).
Una firma sin cuerpo no puede tener docstring: una cadena final sería ella misma el valor de retorno de una
implementación por defecto, así que no se podrían distinguir.

Los traits que ofrece la biblioteca estándar: **`Iter`** (`next` / tipo asociado `Item`; la base de `doiter` y
de las funciones de secuencia), **`Eq`** (`equals`; `not-equals` es una implementación por defecto),
**`Ord`** (hereda `Eq`; solo hay que implementar `less`, y `less-equal` / `greater` / `greater-equal` son
implementaciones por defecto), **`Error`** (`message` / `source`; `:dyn Error` para manejar los tipos de error
de forma uniforme), **`print-object`** (una representación impresa por tipo), **`Pathish`** (designadores de
nombres de ruta: una cadena o un `pathname`) y la jerarquía de streams **`Stream`** → **`InputStream`** /
**`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**. Qué tipos implementan qué traits
está en [types.md](types.md); los métodos de cada trait, en [Traits estándar](functions/traits.md),
[Tipos de error](functions/option-result.md#3-tipos-de-error-y-el-trait-error),
[print-object](functions/printing.md#5-print-object-representación-impresa-por-tipo) y
[Streams](functions/streams-files.md). Si haces `impl` de `Iter` para tu propio tipo de colección, `doiter`
(capítulo 5) y `map` / `filter` / `sort` y similares funcionan sobre él tal cual.

Las llamadas a traits son **estáticas** por defecto (se resuelven según el tipo estático del receptor). Para
manejar valores cuyo tipo concreto se decide en tiempo de ejecución, el tipo de objeto trait `:dyn Trait`
(capítulo 2) da despacho dinámico mediante una vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; un punto de llamada, una respuesta por implementación
```

Solo se pueden hacer `:dyn` los traits en los que "todo método tiene un receptor `self`, no usa `Self` en
ningún sitio salvo en el receptor, y no es él mismo ni genérico ni variádico" (los métodos heredados deben
cumplir las mismas condiciones).

Solo los tipos cuyos valores tienen representación en el montículo pueden ir en una caja `:dyn`:

| Pueden ir | No pueden ir |
|---|---|
| Los tipos `defstruct` / `defenum` (incluidos `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` y las estructuras de la biblioteca estándar), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Los enteros de ancho fijo (`i8` a `u32`), `f32`, `bool`, `char`, `symbol`, `()`, los tipos de función y `Option<T>` sin caja ([la representación en tiempo de ejecución de Option](functions/option-result.md#2-la-representación-en-tiempo-de-ejecución-de-optiont)) |

Colocar un valor de un tipo que no puede ir donde se espera un `:dyn` es un error de tipos. Para manejar esos
valores mediante `:dyn`, envuélvelos en una estructura, como en `(defstruct flag (v bool))`.

### 3.10 module / use — espacios de nombres

```lisp
(module path body...)      ; path es una secuencia de segmentos como foo o foo::bar
(in-module path)           ; desde aquí hasta el final de esta unidad, dentro de path (la forma plana de module)
(use path...)              ; crea alias de funciones, tipos y módulos en el espacio de nombres actual
(pub use path...)          ; hace que lo traído se alcance también por la ruta de este módulo
(import path...)           ; lo mismo que use (una escritura compatible con CL)
(shadowing-import path...) ; un use que toma a sabiendas un nombre simple ya en uso
```

- `module` crea un espacio de nombres. **Los tipos no son espacios de nombres** (como en Rust, un tipo solo
  tiene métodos estáticos y métodos).
- Hacer `use` de un tipo hace que las variantes de un enum y los métodos estáticos públicos del tipo también
  estén disponibles por nombre desnudo (por ejemplo, tras `(use Option)`, `some`/`none` se pueden llamar sin
  `Option::some`/`Option::none`).
- **Un nombre tiene un solo significado.** Las funciones, las variables globales y las macros comparten un
  espacio de nombres, y dentro de un módulo dos de ellas no pueden tener el mismo nombre (es un error sea
  cual sea la que se defina primero). Funciones y variables no tienen espacios de nombres separados como en
  CL. Para el mismo nombre en módulos distintos, es visible el más cercano.
- Resolución de un nombre desnudo `n` (en la posición de llamada `(n a ...)`; los nombres de formas
  especiales se fijan antes de esto):
  1. Locales (parámetros, y los enlaces de `let`, `labels`, `macrolet` y `symbol-macrolet`)
  2. Si el nombre de módulo `n` es una macro, esa macro
  3. Las variantes de un enum incorporado con `use`, y los métodos estáticos de un tipo incorporado con
     `use`
  4. El método `n` del tipo del primer argumento (los métodos se buscan **solo por el tipo del primer
     argumento**)
  5. Si el nombre de módulo `n` es una función o una variable global, eso (para una variable global, se
     llama a la función que es su valor)

  Un nombre de módulo se busca en las definiciones propias del módulo actual, luego en los alias de `use`
  y luego en los nombres que ofrece la biblioteca estándar, y queda fijado por el primero que se
  encuentre. Los módulos padre no se buscan. Para usar una definición de un módulo padre, escribe una
  definición que lleve `pub` como ruta desde la raíz, como `::geo::f`, o tráela con `use`. La misma regla se aplica donde una
  función se pasa como valor, y "el tipo del primer argumento" del paso 4 es el tipo del primer parámetro
  del tipo de función esperado allí (escribir `norm` donde se espera `(fn (pt) int)` da el método `norm` de
  `pt`). Si el tipo esperado aún no está fijado, se omite el paso 4.
- **Los nombres que ofrece la biblioteca estándar** se pueden usar desde cualquier módulo sin `use`:
  los tipos primitivos, `Option`/`Result`/`Vector`, la iteración, las expresiones S y las listas, las
  funciones de secuencias, las macros de control, la comparación y los traits aritméticos; todo nombre
  que también lo es del paquete `COMMON-LISP` de Common Lisp; y los tipos que estos mencionan. Cualquier
  otro nombre — entre ellos las utilidades de concurrencia, red y archivos — se trae con un `use`, como
  `(use std::sync::Chan)`, o se escribe con su ruta, `std::sync::Chan` (la lista está en
  [Nombres que necesitan un `use`](functions/README.md#nombres-que-necesitan-un-use) de las funciones incorporadas). `use` trae funciones,
  variables globales, macros, tipos, traits, alias de `deftype` y módulos. Un `use` solo vale en el
  módulo donde se escribe: para usar un nombre dentro de un `(module ...)`, se escribe el `use` dentro
  de él.
- Una ruta calificada `a::...::n` se resuelve a un nombre de módulo (macro, función o variable global) del
  módulo que nombra `a::...`, o a una variante o un método estático de un tipo. `x::f` en una posición de
  valor (donde `x` nombra un valor) es una lectura de campo.
- **`use` afecta a las formas que van detrás.** Un archivo se lee una forma cada vez, y las dependencias se
  resuelven justo antes de comprobar la forma, así que escribir `m::f` **por encima** de `(use m)` da
  `unresolved path`. Pon `use` al principio del archivo.
- **`use` puede recibir varias rutas** (`(use a::f b::g)`). `import` es una escritura compatible con CL con el
  mismo comportamiento.
- **Se informa de un `use` cuyo nombre simple ya está en uso.** Al resolver un nombre simple se miran las
  propias definiciones del módulo antes que los alias, así que `(use m::twice)` tras `(defun twice ...)` **no
  hace nada**. Si es lo que quieres, escribe `shadowing-import` (aun así no puede ganar a una definición, ya que
  no hay forma de quitarla; solo gana a los alias anteriores).
- **`in-module` es la forma plana de `(module path body...)`.** Escribir `(in-module geometry)` pone todo desde
  ahí hasta el final de la unidad (el archivo, o el cuerpo del `module` envolvente) dentro de `geometry`. Va
  **dentro** del módulo propio del archivo (`main::geometry` para `main.typl`). Dos seguidos se anidan en
  orden. Es distinto del `in-package` de CL, y tiene un nombre distinto: en este sistema el archivo ya es un
  módulo, así que no hay nada que "seleccionar", y lo único que puede hacer una forma es anidar.

### 3.11 Archivos y módulos (proyectos de varios archivos)

La ruta del archivo relativa a la raíz de fuentes es la ruta del módulo: el contenido de
`<root>/geo/point.typl` queda implícitamente envuelto en el módulo `geo::point` (un directorio también es un
segmento, al estilo de Rust / Python). Un `(module bar ...)` explícito en el archivo se anida **dentro** de él
(`geo::point::bar`), así que la ruta derivada y una declaración explícita nunca chocan.

- **Raíz de fuentes**: coloca un archivo de manifiesto `typelisp.toml` en la raíz del proyecto (puede estar
  vacío; opcionalmente una línea `src = "src"` indica el directorio de fuentes). Se encuentra subiendo desde el
  directorio del archivo de destino. Sin manifiesto, el directorio del archivo de entrada (el directorio actual
  para el REPL) es la raíz.
- **Carga bajo demanda**: cuando `(use geo::point)` se refiere a un módulo aún no cargado, el archivo
  correspondiente (`geo/point.typl`) se carga, se comprueba y se registra automáticamente. `use a::b::c`
  busca primero el prefijo más largo: `a/b/c.typl` → `a/b.typl` → `a.typl` (ya que `c` puede ser un elemento
  dentro de un módulo). Las definiciones visibles desde otros módulos necesitan `pub`
  ([3.13 pub](#313-pub--visibilidad)).
- **Las referencias circulares son errores**: la cadena se informa con la forma
  `circular module dependency: a -> b -> a`.
- **Ejecución**: `typl <file.typl>` ejecuta un archivo (sin argumentos, el REPL). `use` en el REPL resuelve los
  archivos con las mismas reglas.
- **Capacidad de la arena de cons**: `typl --heap-cells N` fija la **capacidad inicial** de la arena de celdas
  cons (65536 por defecto; la forma `--heap-cells=N` también funciona, tanto al ejecutar archivos como en el
  REPL). La arena **crece añadiendo más** cuando se queda corta. El límite de crecimiento es 256 veces la
  capacidad inicial, y una reserva más allá da `heap exhausted`: la capacidad inicial significa "reservar esto
  al principio", y el límite significa "más allá de esto, tratarlo como una fuga".

### 3.12 load — carga plana

```lisp
(load "path")   ; solo en el nivel superior; path es un literal de cadena
```

- **Carga plana** al estilo de CL: lee las formas del archivo de destino **en el espacio de nombres actual** tal
  cual (sin envolverlas en un módulo, a diferencia de `use`). Solo en el nivel superior (dentro del cuerpo de
  una función es un error de tipos).
- `path` es relativa al directorio del archivo que carga (desde el REPL, al directorio de trabajo del proceso).
  Si no tiene extensión, se añade `.typl`.
- Los `(load ...)`/`(use ...)` del archivo cargado también se procesan recursivamente.
- **Lee una forma cada vez y la ejecuta en el acto** (como hace el `load` de CL). La forma *k* ha terminado de
  ejecutarse antes de leer la *k+1*: aunque haya un error de sintaxis o de tipos a mitad, las formas
  anteriores ya se han ejecutado. Los archivos de módulo cargados por `use` son distintos: se comprueban como
  una unidad y su ejecución se deja a quien los usó con `use` (corresponde al `compile-file` de CL).

### 3.13 pub — visibilidad

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftrait ...)
(pub deftype ...)
```

`pub` solo se puede poner en los once tipos de arriba y en `use` (no en `module`/`impl`).
`(pub use path...)` **reexporta** lo que trae: otros módulos lo alcanzan por una ruta que pasa por este
módulo (`m::name`) y lo traen con `(use m::name)`. Solo se puede reexportar una definición; un módulo,
no.
Se escribe
con la palabra clave de la definición justo después de `pub`, no en la forma `(pub (defun ...))` que envuelve
la definición entre paréntesis. Un `pub` hace pública exactamente una definición (no se pueden marcar varias
a la vez).

Los métodos de un `impl` inherente se hacen públicos elemento por elemento, como `(pub name ...)` (3.5). Los
campos de una estructura se escriben `(pub x i32)` (3.6).

### 3.14 defmacro — definición de macros

```lisp
(defmacro name (obligatorios... &optional opt... &rest rest-name &key key...) body...)
```

- Todos los parámetros y el valor de retorno son siempre `Sexpr`, así que no se escriben anotaciones de tipo.
- Macros no higiénicas al estilo de CL (evitar choques con `gensym` es responsabilidad de quien escribe la
  macro).
- La lista lambda sigue el orden de CL `obligatorios &optional &rest &key` (cada marcador como mucho una vez, y
  solo en este orden).
  - `&optional` … argumentos opcionales. `name` o `(name expr-por-defecto)`. La expresión por defecto se evalúa
    al expandir (puede referirse a parámetros ligados antes) y se liga cuando se omite el argumento (sin valor
    por defecto, la lista vacía `()`).
  - `&rest name` … recibe los argumentos posicionales restantes juntos como una lista `Sexpr`.
  - `&key` … argumentos de palabra clave. `name` o `(name expr-por-defecto)`. Quien llama los pasa como
    `:name valor` (en cualquier orden). Si se omiten, la expresión por defecto (la lista vacía `()` si no la
    hay). Las palabras clave desconocidas o una secuencia `:key` de longitud impar son errores.
- Ejemplos: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — enlaces de macros locales

```lisp
(macrolet ((name (lista-lambda) body...) ...) body...)   ; macros de alcance léxico
(symbol-macrolet ((name expansión) ...) body...)          ; un nombre representa una forma
```

Ambas son formas especiales de **expresión**, y no queda nada en tiempo de ejecución (lo que se compila es la
forma expandida del cuerpo). La lista lambda es la misma que la de `defmacro`. Las reglas detalladas y los
ejemplos están en
[Enlaces de macros locales](functions/system.md#9-enlaces-de-macros-locales-macrolet--symbol-macrolet).

### 3.16 `#[test]` — funciones de prueba

```lisp
(use std::error::assert-eq)
#[test]
(defun adds () ()
  (assert-eq (+ 1 2) 3))
```

Un `defun` marcado con `#[test]` es una función de prueba, que `typl test` llama de una en una (igual que `#[test]` de Rust). No debe recibir argumentos y debe devolver `()` o `Result<(), E>` (tampoco puede recibir parámetros de tipo). No necesita `pub`. Una prueba que devuelve un `Result` falla al devolver un `err`, y el error se muestra, así que `E` debe implementar `Error` o `print-object` (igual que Rust pide `Debug` al tipo de error de una prueba). Un `Error` se muestra con `describe-error` (el mensaje y su cadena de causas), y cualquier otra cosa con `print-object`. `:dyn Error` también es un `Error`, así que una prueba puede reunir errores distintos en un solo tipo y devolverlo.

```lisp
(use std::num::ParseIntError std::num::parse-int)
#[test]
(defun parses () Result<(),ParseIntError>
  (match (parse-int "12")
    ((ok _) (Result::ok ()))
    ((err e) (Result::err e))))
```

- **Una ejecución normal y `compile-file` la omiten**, sin siquiera comprobar su contenido (igual que Rust compila las pruebas solo con `--test`). El propio programa no puede llamar a una función de prueba.
- Una prueba puede escribirse en cualquier archivo que contenga funciones, dentro de cualquier `module`. También pueden guardarse aparte en la carpeta `tests/` del proyecto.
- Una prueba falla por un `panic`, por un error en tiempo de ejecución o al devolver un `err`. Se comprueba con `assert`, o con `assert-eq`, que imprime ambos lados ([Sistema](functions/system.md)). Aunque una prueba falle, las demás siguen ejecutándose.

```sh
typl test                 # ejecuta todas las pruebas del proyecto de esta carpeta
typl test parse           # solo aquellas cuyo nombre (el nombre completo, como `tests::more::parse-int`) contiene parse
typl test foo.typl        # las pruebas de foo.typl (y de los archivos que usa)
typl test foo.typl parse  # de esas, las que tienen parse en el nombre
```

En un proyecto (el `typelisp.toml` que se encuentra subiendo por las carpetas), `typl test` carga todos los archivos `.typl` bajo la carpeta de fuentes y todos los archivos `.typl` bajo la carpeta `tests/` junto a `typelisp.toml`. El módulo de un archivo bajo `tests/` lo da su ruta desde la carpeta de `typelisp.toml` (`tests/parse.typl` es `tests::parse`).

`typl test` **no ejecuta las expresiones de nivel superior** de los archivos que carga. Ese es el trabajo propio del programa, que una ejecución de pruebas no hace (igual que `go test` no llama a `main`). Las definiciones (`defun`, el valor inicial de un `defvar`, etc.) y los registros de macros de lector se hacen como siempre. Si hay un error de tipos en cualquier parte, no se ejecuta ninguna prueba.

El resultado es una línea por prueba, con los motivos de los fallos reunidos al final. Si falla aunque sea una prueba, el código de salida es 1.

```text
running 2 tests
test math::adds ... ok
test math::adds-wrong ... FAILED

failures:

---- math::adds-wrong ----
src/math.typl:9:3: panic: assertion failed: (assert-eq (add 1 2) 4)
  left: 3
 right: 4

test result: FAILED. 1 passed; 1 failed; 0 filtered out
```

Lo que una prueba imprime con `print` va directamente a la salida estándar (no se captura). Para la salida destinada a investigar, `eprint`/`eprintln` ([Impresión](functions/printing.md)) escriben en la salida de error estándar y no se mezclan con la salida del programa.

## 4. Enlaces y condicionales

```lisp
(let ((name val) ...) body...)      ; enlace en paralelo
(let* ((name val) ...) body...)     ; enlace secuencial (los enlaces anteriores se pueden usar en los inicializadores posteriores)

(if cond then else)                 ; else es obligatorio (siempre tres elementos)
(when cond body...)                 ; un if sin else (tipo Unit). defmacro
(unless cond body...)               ; la negación de when. defmacro
(cond (test1 body...) (test2 body...) ... (:else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; una lista de claves: coincide si coincide alguna
  (:else body...))                   ; expr se evalúa una vez. las claves se comparan con equal.
                                     ; las claves son "literales" y no se evalúan (como en CL).
                                     ; un símbolo simple a significa el símbolo 'a.
                                     ; escribir 'a es un error (usa la a simple). defmacro
(ecase expr (key body...) ...)      ; un case que exige coincidencia. panic si no coincide nada. defmacro
(ccase expr (key body...) ...)      ; el ccase de CL. no hay reinicios que ofrecer, así que es igual que ecase. defmacro
(and expr...)                       ; evaluación en cortocircuito. true con cero argumentos. defmacro
(or expr...)                        ; evaluación en cortocircuito. false con cero argumentos. defmacro
(progn body...)                     ; ejecuta en orden y devuelve el último valor
(unsafe body...)                    ; lo mismo que progn, más el permiso para escribir llamadas FFI
                                     ; y palabras en bruto. consulta 3.3 defffi
(prog1 form more...)                ; lo evalúa todo; el valor es el de form. defmacro
(prog2 a b more...)                 ; lo evalúa todo; el valor es el de b. defmacro
(the Type expr)                     ; una anotación de tipo (sin efecto en tiempo de ejecución)
```

### 4.1 unsafe — asumir lo que no se puede comprobar

```lisp
(unsafe body...)
```

Lo mismo que `progn`: evalúa el cuerpo en orden y devuelve el último valor. No crea ningún ámbito ni es una
frontera de función (`break` / `return-from` lo atraviesan directamente hacia fuera). La diferencia es que
algunas cosas solo se pueden escribir dentro de él.

Actualmente tres cosas requieren `unsafe`: llamar a funciones de C declaradas con
[defffi](#33-defffi--declarar-funciones-de-c-ffi), convertir en valores las palabras de máquina en bruto (`ptr`
/ `c-long` / `c-ulong` / `(ptr T)`), y [`def-c-struct` y `c-alloc`](#def-c-struct-y-punteros-tipados--reservar-structs-de-c).

La memoria reservada con `c-alloc` se libera al salir del `unsafe` más externo dentro de la misma función.
Solo ese `unsafe`, a diferencia de `progn`, tiene trabajo que hacer al salir: la liberación.

Lo que asume `unsafe` son las siguientes suposiciones que el compilador no puede verificar:

- **Que los tipos coinciden.** Que la firma de C declarada coincide con la real. Si no, los argumentos van a
  los registros equivocados y los valores de retorno se leen con el ancho equivocado.
- **La seguridad de memoria.** Qué hace el lado de C con lo que recibe.
- **El estado de todo el proceso.** Variables de entorno, manejadores de señales, `errno`. Por ejemplo, llamar
  a `setenv` mediante el FFI rompe las suposiciones que hace el `decode-universal-time` de esta implementación
  cuando calcula la hora local.
- **La seguridad entre hilos.**

No es una salida de la comprobación de tipos. `(unsafe (+ 1 "two"))` no pasa. Lo que se permite es escribir
ciertas **operaciones**, no escribir sinsentidos.

Funciona léxicamente. El cuerpo de una `lambda` escrita dentro de `unsafe` hereda el permiso (como con los
cierres dentro de los bloques `unsafe` de Rust). El valor puede llamarse más tarde desde fuera del `unsafe`,
pero escribirlo ahí ya se toma como aceptar la responsabilidad.

### 4.2 destructuring-bind — desarmar listas por su forma

```lisp
(destructuring-bind lista-lambda form body...)
```

Desarma **por su forma** la lista que produce `form` y la liga. La lista lambda es la de `defmacro`
(obligatorios → `&optional` → `&rest`/`&body` → `&key`, cada uno con expresiones por defecto), por el mismo
motivo por el que CL comparte una entre ambas: son dos formas que desarman lo mismo.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Toda variable ligada es un `Option<Sexpr>`.** No es una limitación de la implementación sino la naturaleza
  de lo que se liga: las listas de expresiones S son las únicas listas de este lenguaje, así que no hay otro
  tipo que dar a los elementos. Recurrir a `match` donde hace falta un escalar es igual que en un cuerpo de
  `defmacro`.
- **Una forma que no coincide provoca un panic** (corresponde al error de CL): demasiados o demasiado pocos
  elementos, una secuencia `&key` de longitud impar o una palabra clave desconocida. `sexpr-car` es una función
  permisiva que devuelve `()` para `()`, así que sin la comprobación, una lista corta se ligaría en silencio a
  una secuencia vacía.
- **No se admiten listas lambda anidadas.** `defmacro` tampoco las admite, así que hay una sola regla.
  `(a (b c))` no liga en silencio una sublista a `b`; es un error que lo dice.
- Las expresiones por defecto de `&optional` / `&key` **solo se evalúan cuando se usan** (como en CL).
- No hay nada que corresponda al `&allow-other-keys` de CL (`defmacro` tampoco lo tiene).

### 4.3 match — coincidencia de patrones

```lisp
(match expr
  (pattern body...)
  ...)
```

Clases de patrones:
- `_` — comodín
- Un nombre de variable — un patrón de enlace (siempre coincide). Sin embargo, si el tipo del valor examinado
  tiene una variante con ese nombre, se resuelve como **el patrón de nombre de variante simple de abajo**
- Un nombre de variante simple — coincide con una variante que no recibe argumentos
  (`(match c (red 1) (blue 2))`). Escribir una variante con campos por su nombre simple es un error de aridad,
  así que escríbela entre paréntesis, como en `(circle r)`
- **Literales inmediatos**: enteros / `true`/`false` / caracteres — se comparan como palabras
- **Literales de valor**: cadenas / números de coma flotante / símbolos (`'foo`) / enteros bignum / ratios — se
  comparan por valor con el `Eq::equals` de ese tipo ([Traits estándar](functions/traits.md#2-eq--ord-comparación)).
  Las cadenas se comparan por contenido, no por identidad
- `(:= expr)` — evalúa cualquier expresión y compara con `Eq::equals`. La única forma de comparar tipos que no
  tienen sintaxis literal (instancias de `defstruct`, globales, resultados calculados), y una implementación de
  `Eq` definida por el usuario se convierte tal cual en la regla de comparación. `expr` se puede referir a
  cualquier cosa visible desde la posición de la rama (argumentos, enlaces exteriores, globales)
- `(Ctor sub-pattern...)` — patrones de constructor (`(some x)` `(none)` `(cons a d)` `(ok v)`, etc.)
- `(Type :slot pattern... :slot...)` — patrones de estructura. Escribe las ranuras después del nombre del
  tipo como palabras clave. Si a una palabra clave le sigue un patrón, esa ranura se compara con el patrón;
  si le sigue otra palabra clave o nada, la ranura se enlaza a una variable con su propio nombre. Las
  ranuras no escritas no se comparan (así el patrón no depende del número ni del orden de los campos):

  ```lisp
  (defstruct point (x i32) (y Option<i32>))

  (defun f ((p point)) i32
    (match p
      ((point :x :y (some b)) (+ x b))   ; enlaza x, y compara y con (some b)
      ((point :x a) a)))                 ; enlaza a
  ```
- `#{p0 p1 ...}` — un patrón de tupla. Cada elemento se compara con su propio subpatrón. Frente a un
  `Sexpr`, solo coincide con datos `#{..}` del mismo número de elementos
- `(:or p1 p2 ...)` — un patrón o: coincide cuando coincide cualquiera de las alternativas. Hay un
  solo cuerpo, así que cada alternativa debe enlazar las mismas variables con los mismos tipos.
  También puede escribirse dentro de un patrón de constructor (`(some (:or (circle r) (rect r _)))`)

Comparar un tipo que no implementa `Eq` con un literal de valor / `(:= expr)` es un error de tipos (este lenguaje
prefiere decir "no se pueden comparar" a dejar una rama que en silencio nunca coincide).

**Guardas**: escribir `:when condición` tras el patrón elige la rama solo cuando el patrón coincide
y además la condición es verdadera. Si la condición es falsa, se prueba la rama siguiente. La
condición puede leer las variables que enlazó el patrón.

```lisp
(defun classify ((n int)) string
  (match n
    (0 "zero")
    (k :when (< k 0) "negative")
    (k :when (evenp k) "even")
    (_ "odd")))
```

- Una rama con guarda no cuenta para la exhaustividad (como en Rust). Su condición puede ser falsa,
  así que las variantes que cubriría necesitan además una rama sin guarda o un `_`.
- Los patrones o y las guardas se combinan. La guarda se evalúa sea cual sea la alternativa que
  coincidió (`((:or 1 2 3) :when on "small")`).

**Literales de valor contra un valor examinado `Sexpr`**: el `Eq` de `sexpr` es `eq` (la identidad de CL), así
que los inmediatos (`'foo` (internado) / enteros / caracteres / `true`/`false`) se pueden escribir tal cual y
coinciden por contenido:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Los literales no inmediatos (cadenas / números de coma flotante / enteros bignum / ratios) **no se pueden
escribir** contra un `Sexpr`. Su `eq` compara la identidad del objeto, lo que daría "una rama que supera la
comprobación de tipos pero nunca coincide", así que es un error que nombra el patrón de variante: escribe
`(str "hi")` y se desarma en un `string` que se compara por contenido. `(:= expr)` pide explícitamente `equals`,
así que esta restricción no se le aplica.

**El valor examinado no tiene por qué ser un ADT.** `string`/`symbol`/`i32`/`f64` y similares se pueden pasar
a `match` directamente (ahí es donde van los patrones de literales de cadena). Sin embargo, un tipo sin
variantes no se puede cubrir por enumeración, así que `_` (o un patrón de enlace que actúe de comodín) es
obligatorio:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; un tipo sin variantes necesita `_`
```

Contra un valor examinado `Sexpr`, además de los 19 patrones de variante incorporados de arriba, se pueden
escribir **patrones de conversión descendente** (para sacar instancias de ADT definidos por el usuario):
sintaxis para recuperar, con `match`, una instancia de un `defstruct`/`defenum` (capítulo 3) que se convirtió
implícitamente en `Sexpr`, como en `(list p 42)`:

- `(TypeName :slot pattern...)` — descomposición de campos con el **nombre del tipo** primero (solo
  structs), escrita en la misma forma que los patrones de estructura de arriba. Por ejemplo, para
  `(defstruct point (x f64) (y f64))`, `(point :x :y)`.
- Un nombre de variante simple `(VariantName sub-pattern...)` — extrae una variante de un `defenum`. Se resuelve
  como un nombre simple visible tras `(use EnumType)` (las mismas reglas de visibilidad que al llamar al
  constructor). Por ejemplo, para `(defenum color (red) (blue))`, `(red)` `(blue)` tras `(use color)`. Si los
  nombres de variante de varias enumeraciones visibles chocan, es un error de ambigüedad, así que también se
  puede escribir la forma calificada `(EnumType::VariantName ...)` (no hace falta `use`).
- `(the Type pattern)` — una conversión descendente del tipo entero (ligándolo entero). No descompone los
  campos; pasa el valor a `pattern` tal cual. La única forma de sacar una estructura mutable conservando su
  identidad, y también la única forma de sacar un `Vector<T>`/`HashTable<K,V>` de un `Sexpr` (no tienen forma
  de descomposición por campos). Por ejemplo, tras `(the point p)`, `(setf p::x 9)` también se refleja en la
  instancia original de la lista.

**Patrones para `Option<Sexpr>`**: el tipo de los datos de expresiones S no es `Sexpr` sino `Option<Sexpr>`, y
la lista vacía no es una variante de `Sexpr` sino el `none` de `Option`. Así que al hacer `match` de un
`Option<Sexpr>`, las 19 variantes de `Sexpr` y `none` se pueden escribir **planas en la misma lista de ramas**
(no hace falta un `match` exterior que quite el `Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; la lista vacía
    (_          9)))
```

La exhaustividad se comprueba en el mismo universo plano: las 19 variantes de `Sexpr` más `none`, 20 en total.
Olvidar `(none)` es un error salvo que haya un `_`. También se puede escribir `(some x)`, que liga "algo no
vacío".

Esta facilidad se aplica **exactamente** solo a `Option<Sexpr>`. Para `Option<Option<Sexpr>>`, no estaría claro
qué capa quitó `(int n)`, así que escribe dos niveles de `match` como de costumbre.

Los mismos patrones de conversión descendente se pueden usar tal cual sobre **un valor examinado que es un
objeto trait (`:dyn Trait`, capítulo 2)**: `match` lo saca de la caja y después lo entrega a la maquinaria de
patrones de `Sexpr` de arriba, así que no hay sintaxis adicional. El conjunto de tipos que lo implementan es
abierto, así que nunca puede ser exhaustivo, y `_` es obligatorio:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle :r) (* (* r r) 3))    ; descomposición por campos con el nombre del tipo primero
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Inferencia de tipos entre ramas**: todas las ramas deben tener el mismo tipo (salvo las ramas que divergen,
como con `panic`). En un `match` escrito donde no se espera ningún tipo, las ramas se completan mutuamente los
argumentos de tipo que les faltan: `(Result::ok v)` fija solo `T`, y `(Result::err e)` solo `E`, pero juntas
fijan `Result<T,E>`. Un argumento de tipo que ninguna rama puede fijar al final es un error de esa rama
(`cannot infer type argument ...`). Fuera de `match`, un argumento de tipo que no se puede fijar es un error en
el acto.

La comprobación de exhaustividad de un `match` que usa patrones de conversión descendente no los cuenta para la
cobertura de las propias variantes de `Sexpr` (un `match` que solo enumera patrones de conversión descendente
debe cerrarse con `_`). Para los ADT genéricos (`defstruct point<T> ...`, etc.), los argumentos de tipo de un
patrón de conversión descendente no se pueden inferir, así que no se pueden usar la forma de descomposición por
campos (`(point ...)`) ni la forma de variante simple; indícalos con `the`, como en `(the point<i32> p)`.

**Las conversiones descendentes también miran la instanciación.** Los argumentos de tipo explícitos se usan
para emparejar: `(the point<i32> p)` solo deja pasar valores de `point<i32>`, y un `point<string>` pasa a la
siguiente rama. Esto se debe a que un valor recuerda su tipo incluidos sus argumentos de tipo (el mismo
mecanismo que elige `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (con enlaces) si val coincide con pattern; si no, els. defmacro
(while-let (pattern val) body...)   ; repite mientras val (reevaluado cada vez) coincida con pattern. defmacro
```

## 5. Iteración

```lisp
(loop body...)                      ; un bucle infinito. se sale con break/return
(while test body...)                ; repite mientras test sea verdadero. defmacro
(until test body...)                ; repite mientras test sea falso (la negación de while). defmacro
(dotimes (var count-expr) body...)  ; evalúa count-expr una vez y recorre var de 0 a count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; iteración al estilo de CL con avance en paralelo. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; la versión secuencial de do (enlace let*, asignación en orden). defmacro
(doiter (var coll-expr) body...)    ; itera sobre un valor que implementa el trait Iter. defmacro

(break)                             ; sale solo del bucle más interno. el valor es siempre Unit
(return)                            ; sale solo del bucle más interno
(return value)                      ; sale del bucle más interno con un valor
```

Tanto `break` como `return` salen **solo del bucle envolvente más interno** (no son un retorno anticipado de la
función, y no pueden cruzar la frontera de una `lambda`). El tipo de un `loop` es la unión de los tipos de valor
de los `break`/`return` que hay dentro (`!` si nunca se sale). Para salir de una función, usa `return-from`,
abajo.

### 5.1 `block` / `return-from` — salidas con nombre

```lisp
(block name body...)                ; un destino de salida con nombre. el valor es la última forma,
                                    ; o el valor que pasa return-from
(return-from name)                  ; sale de ese bloque con Unit
(return-from name value)            ; sale con un valor
```

**Cada función de `defun` / métodos de `impl` / `labels` establece implícitamente un bloque con su propio nombre**
(como en CL). Así que `(return-from f v)` es un retorno anticipado de la función:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` es una salida **léxica**, y el nombre **se resuelve donde se escribe**: el comprobador asocia un
`return-from` con el `block` envolvente y une el tipo de su valor al tipo de salida del bloque. Así que:

- Un `return-from` sin `block` correspondiente es un **error de tipos** (no un error en tiempo de ejecución).
- Un valor cuyo tipo no encaja con las demás salidas o con el tipo del cuerpo es un **error de tipos** (la
  misma regla que para las ramas de `match`).
- Si se anidan bloques con el mismo nombre, **gana el interior** (la regla de ocultación de CL).
- **No puede cruzar fronteras de función.** Desde dentro de una `lambda` no se puede salir a un `block`
  exterior (`lambda` no establece ningún bloque: los bloques implícitos de CL necesitan un *nombre*, y las
  funciones anónimas no lo tienen). Lo que necesita cruzar es `catch`/`throw` (capítulo 8, que es
  **dinámico**).

Como `break`/`return` (capítulo 5), es una salida **estática**, así que en el código compilado es un salto a un
bloque básico fijado en tiempo de compilación. Si hay un `unwind-protect` en medio, se ejecuta su `cleanup`
(capítulo 8).

Si nunca escribes `return-from`, el bloque implícito no cuesta nada.

### 5.2 `loop` extendido (el LOOP de CL)

**Si el primer elemento de `loop` es una palabra clave**, se lee como una secuencia de cláusulas. Si no, sigue
siendo el bucle simple de arriba, y el significado de los `loop` existentes no cambia (lo mismo que la regla del
bucle simple del propio CL).

CL escribe las palabras de cláusula como símbolos simples (`(loop for i from 1 to 3 collect i)`), pero aquí
**todas son palabras clave**: un `for` simple sería solo una referencia a una variable, y ser una palabra clave
es también lo que lo distingue de un bucle simple. La palabra que separa una variable de su valor no es una excepción: se escribe `:=`.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x := 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Cláusulas de variable** (se escriben antes de las cláusulas de cuerpo. Es la regla de CL: escritas después,
podrían leerse como "iterar solo a partir de ahí", así que es un error):

| Cláusula | Significado |
|---|---|
| `:with v := e` | Liga una vez. Puede leer las variables de cláusulas anteriores |
| `:for v :in s` / `:for v :across s` | Los elementos de un `Iter` en orden. La distinción lista/vector de CL no existe aquí, así que son dos escrituras de la misma cláusula |
| `:for v :on s` | Los **sufijos** sucesivos. CL pasa el cons de cola compartido, pero un `Iter` no tiene cola que compartir, así que cada uno es un `Vector` nuevo |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Contar. También funcionan `:downfrom`/`:upfrom` |
| `:for v := e [:then f]` | Empieza con `e`, y a partir de la segunda vez usa `f` (sin `:then`, `e` cada vez) |
| `:repeat n` | Itera ese número de veces |

Con varios `:for`, avanzan **en paralelo**, y el bucle termina en cuanto se agota cualquiera de ellos.

**Cláusulas de cuerpo** (se ejecutan cada vez, en el orden en que se escriben):

| Cláusula | Significado |
|---|---|
| `:do form...` | Para efectos secundarios |
| `:collect e [:into v]` | Recoge en un `Vector<T>` |
| `:append e [:into v]` | Añade el contenido de un `Iter` |
| `:sum e` / `:count e` | La suma / el número de veces que fue verdadero |
| `:maximize e` / `:minimize e` | El máximo / el mínimo. **`Option<T>`** (igual que CL devuelve nil para una secuencia vacía; un tipo `Ord` arbitrario no tiene elemento mínimo) |
| `:always e` / `:never e` | `true` si se cumplen todos; `false` de inmediato en cuanto falla uno |
| `:thereis e` | `e` es un **`Option<T>`**. Devuelve el primer `some`, o `none` si no hay ninguno (esto es lo que corresponde al "primer valor no nil" de CL; para probar un `bool`, usa `:always`/`:never`) |
| `:while e` / `:until e` | **Termina normalmente** aquí (`:finally` se ejecuta, y lo recogido es la respuesta) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Hace condicional una cláusula |
| `:return e` | Sale de inmediato con ese valor (`:finally` no se ejecuta, como en CL) |
| `:initially form...` / `:finally form...` | Antes del bucle / al terminar normalmente |

**`:named name`** (antes de cualquier otra cláusula, solo una vez) envuelve todo el bucle en `(block name …)`.
`(return-from name e)` puede salir de una vez incluso desde dentro de bucles anidados, y como `:return`,
`:finally` no se ejecuta. Sin nombre, no se establece ningún bloque: el `loop` sin nombre de CL establece
`block nil`, pero aquí no hay `nil`, y `break`/`return` (capítulo 5) ya ofrecen "salir del bucle más interno".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Omitir `:finally (return 0)` es un **error de tipos**. Son simplemente las reglas de `block` en acción (5.1): el
tipo de la salida `int` no encaja con el `()` que deja el bucle cuando se agota.

**El valor del bucle** es la acumulación de la cláusula acumuladora si la hay (la primera, si hay varias),
`true` para `:always`/`:never`, `none` para `:thereis`, y `()` si no hay ninguna. Si lo último de `:finally` es
`(return e)`, ese es el valor: el modismo `finally (return …)` de CL, la única forma en que un bucle que no
acumula puede indicar su propia respuesta.

**Diferencias con CL / lo que no se incluye**:

- **Las palabras de cláusula son palabras clave** (arriba).
- `:maximize`/`:minimize`/`:thereis` devuelven `Option<T>` (no hay nil).
- **Escribir solo `:return`, sin acumulación ni `:finally`, es un error.** CL devuelve nil al agotarse, pero aquí
  no existe tal cosa, así que el bucle tiene que decir cuál es su valor al agotarse.
- No se incluyen unir cláusulas paralelas con `:and`, `:being`/la iteración dedicada sobre tablas hash, `:it`
  ni `:nconc`.
- El tipo de los elementos de `:collect` sale del tipo de la expresión acumulada. Intentar recoger un tipo que
  **no se puede escribir como nombre de tipo**, como un tipo de función, es un error que lo dice.

## 6. Valores de función y llamadas

```lisp
(lambda (params) RetType body...)   ; crea un valor de función de primera clase (un cierre)
(labels ((name (params) RetType body...) ...) body...)   ; definiciones de funciones locales que pueden ser mutuamente recursivas
(apply f arg1 ... argN rest-list)   ; llama a f (una función variádica con &rest), desplegando rest-list
```

Las funciones con nombre también se pueden pasar como valores tal cual (como argumentos de funciones de orden
superior, etc.).

## 7. Otras formas especiales

```lisp
(setq var value ...)                ; la asignación de variables de CL. solo una secuencia de (setf var value). defmacro
(psetq var value ...)               ; asignación en paralelo. evalúa primero todos los valores y después asigna. defmacro
(psetf place value ...)             ; psetq generalizado a lugares (la misma expansión). defmacro
(setf place value)                  ; asignación a un lugar. un lugar es un nombre de variable / var::field /
                                     ; una llamada de la forma (accessor recv key...). válida si el
                                     ; tipo estático de recv tiene un método de instancia llamado
                                     ; set-{accessor} (para el get de Vector<T> y HashTable<K,V>,
                                     ; corresponde set como excepción; si no, set-nombre-del-accesor).
                                     ; el valor es el valor asignado (como en CL). así que
                                     ; en (if c (setf x 1) ()), then y else no tienen tipos que coincidan
(incf place)  (incf place delta)    ; place += delta (delta=1 si se omite). el resultado es como con setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 si se omite)
(rotatef place1 place2 ... placeN)  ; rota N lugares (nuevo place1=viejo place2, ...,
                                     ; nuevo placeN=viejo place1). las subformas de cada lugar se evalúan una vez
(shiftf place1 ... placeN newvalue) ; desplaza a la izquierda los valores de place2..N y pone newvalue en placeN.
                                     ; el valor de retorno es el viejo valor de place1
(list e1 e2 ... en)                 ; se expande a (cons e1 (cons e2 (... ()))). () con cero argumentos.
                                     ; cada elemento se convierte implícitamente en Sexpr (como el cons de
                                     ; CL, puede contener cualquier valor). los escalares (int/i32/f64/ratio/
                                     ; char/bool/string/symbol) se envuelven en la variante Sexpr
                                     ; correspondiente, y defstruct/defenum/Vector<T>/HashTable<K,V> y
                                     ; similares entran tal cual (sin coste de conversión). lo mismo para
                                     ; los argumentos de &rest/format.
(source-file)                       ; el nombre del archivo del que se leyó esta forma (string). fijado
                                     ; como constante al comprobar. corresponde al *load-pathname* de CL,
                                     ; pero no es una variable: los cuerpos de módulo se ejecutan tras la
                                     ; comprobación, así que no se puede contar con "lo que se carga ahora",
                                     ; mientras que al comprobar siempre se conoce.
                                     ; para fuentes que no son archivos, el nombre que les da el lector (<stdin>/<input>)
(quote datum)                       ; lo mismo que 'datum. lo devuelve como datos Sexpr sin evaluar
(quasiquote template)               ; lo mismo que `template. incrusta expresiones en la plantilla con ,/,@
(documentation name)                ; devuelve la docstring de name (un nombre simple o Type::method) como Option<string>
(panic message)                     ; message: string. termina de forma anómala con un error irrecuperable. tipo !
(unreachable)                       ; se expande a (panic "unreachable"). defmacro
(todo)                              ; se expande a (panic "todo"). defmacro
(as Type expr)                      ; conversión de tipo numérica/de carácter. las conversiones que pueden fallar provocan un panic si fallan
(try-as Type expr)                  ; como as, pero devuelve el resultado como Option<Type> (none si falla)
(print control args...)             ; expande el formato y escribe en la salida estándar (sin salto de línea)
(println control args...)           ; lo mismo (con un salto de línea al final)
(format dest control args...)       ; el format de CL. devuelve la cadena expandida
(pprint x)                          ; impresión bonita. escribe primero un salto de línea, como en CL
(pprint-fill x)                     ; disposición de relleno
(pprint-linear x)                   ; todo en una línea o un elemento por línea
(pprint-tabular x [colinc])         ; disposición tabular (16 columnas por defecto)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; construir un bloque lógico por tu cuenta
```

La familia `print`/`println`/`format`/`pprint` son formas especiales, así que sus argumentos variádicos (un único
objeto en la familia `pprint`) se envuelven en `Sexpr` con sus propios tipos antes de pasarse: por eso
`(println "~a" my-struct)` simplemente funciona. Los detalles de las directivas de formato y del pretty printer
están en [Directivas de formato](functions/format.md) y en [Impresión](functions/printing.md#4-el-pretty-printer).

`as`/`try-as` solo tratan el catálogo numérico y de caracteres (entre `int`, los tipos enteros de ancho fijo y
`f32`/`f64`/`ratio`/`char`). El mismo tipo no es una conversión. **Las conversiones entre anchos enteros
(incluido `int`) y entre `f32`↔`f64` son conversiones reales**: `as` trunca / redondea, y `try-as` responde si
cabe en ese ancho (precisión). `(as int x)` es la ampliación exacta desde un ancho fijo, y `(as i32 n)` el
truncamiento desde `int`. Entero → `char` puede fallar fuera de rango, así que `as` provoca un panic y `try-as`
da `none`. Todo lo demás (las ampliaciones, y el truncamiento de `float->int`/`ratio->int`) siempre tiene éxito.
`float->int`/`ratio->int`/`char->int` llegan a `int`, y si se pide un ancho más estrecho, se llama después a
`int->W`. Es una facilidad de escritura que se expande a los métodos de conversión correspondientes
(`int->char`/`int->int`/`int->W`, etc. en [Números](functions/numbers.md)).

Desde `rational`, `as`/`try-as` convierten a `int` (`as` trunca, `try-as` solo tiene éxito con un
entero), a `ratio` y a `f64`; desde `int`/`ratio`, a `rational`.

`documentation`, como `quote`/`compile`, es una forma especial que lee `name` sin evaluarlo, como un símbolo
simple / ruta `::` sin evaluar. A diferencia del `(documentation 'name 'function)` de CL, no recibe argumento de
tipo: resuelve `name` en el orden variable → función → tipo → trait → macro (la misma prioridad que cuando un
identificador simple se evalúa como expresión) y devuelve la docstring de la definición encontrada
(`(documentation Type::method)` es para métodos). No poder resolverlo (no hay definición con ese nombre) es un
error al comprobar; una definición que existe pero no tiene docstring da `Option::none`. Todo se decide como
constante al comprobar: no se hace ninguna búsqueda en tiempo de ejecución. No se admiten nombres libres
calificados con módulo (`mod::name`, salvo `Type::method`).

## 8. Salidas no locales (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; ejecuta body. si ocurre (throw 'tag v) en cualquier
                                    ; lugar al que llegue body, ese v se convierte en el valor
(throw 'tag value)                  ; sale al (catch 'tag ...) dinámicamente envolvente más cercano
(unwind-protect protected cleanup)  ; ejecuta cleanup se salga como se salga de protected
```

A diferencia de `break`/`return` (capítulo 5), esta es una salida **dinámica**: `throw` no busca léxicamente el
`catch` que lo rodea, y llega a un `catch` con la misma etiqueta a través de cualquier número de llamadas a
funciones.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; si no se encuentra, el valor del final como de costumbre
```

- **Las etiquetas son solo símbolos literales** (`'done`). A diferencia de CL, no se evalúan.
- **Una etiqueta lleva un tipo.** El tipo se decide la primera vez que se usa `'tag`, y todo `throw`/`catch`
  posterior del mismo símbolo se contrasta con él. Usarla con otro tipo es un error de tipos.
- El tipo de `throw` es `!` (diverge). El tipo de `(catch 'tag expr)` es la unión del tipo de `expr` y el tipo
  de la etiqueta.
- El valor de `unwind-protect` es el valor de `protected`. El valor de `cleanup` se descarta. `cleanup` se
  ejecuta se salga como se salga de `protected`: además de la terminación normal, `throw` y `panic`, también se
  ejecuta cuando se sale por `break`/`return`/`return-from`. Una salida no local del propio `cleanup` gana a la
  salida en curso.
- Los `unwind-protect` anidados se ejecutan de dentro hacia fuera. Un `break` que sale de un bucle **dentro**
  de `protected` no ha salido de `protected`, así que su `cleanup` no se ejecuta.

No se adoptan las condiciones de CL (`define-condition`/`handler-bind`/`invoke-restart`). No encajan con el
tipado estático, así que los fallos recuperables se expresan con `Result` (capítulo 9).

## 9. Política de manejo de errores

- Fallos recuperables: `Result<T,E>` + `match`. Fallos irrecuperables (errores de programación, invariantes
  rotos): `panic`.
- No hay sintaxis que corresponda a `?`/try. Las ramificaciones se escriben explícitamente con `match`.
- Los nombres de funciones y formas especiales no usan `!` (operaciones destructivas) ni `?` (predicados) como
  sufijos. Los predicados se nombran con un sufijo `-p`/`p` (`zerop`, `consp`, etc.) o con un prefijo `is-`
  (`is-some`, `is-ok`, etc.).

## 10. Compilación

```lisp
(compile name)                      ; compila con JIT a código nativo un defun/método ya definido
(compile-file src-path out-path)    ; compila con AOT un archivo fuente a un ejecutable nativo (se salta el `(main)` final)
(dump path)                         ; escribe el entorno actual (información de tipos + cuerpos compilados) en un archivo
(disassemble name)                  ; imprime en qué se convierte esa definición (código máquina del anfitrión por defecto, LLVM IR con true como segundo argumento)
```

`compile` es una forma especial; `name` no se evalúa y se lee como un símbolo simple / ruta `::` sin evaluar
(una cadena es un error de tipos). Las funciones genéricas no pueden ser el objetivo: en cada lugar de uso se
crea una copia para cada tipo, así que no existe un único cuerpo compilado. **Un nombre que no se puede resolver
es un error al comprobar** y nunca se arrastra hasta el tiempo de ejecución (hay mensajes distintos para: el
tipo existe pero no ese método / no existen ni el tipo ni la función / un nombre simple sin definir). La
visibilidad aquí se trata como en cualquier otra referencia: "existe pero no es visible desde aquí" falla al
comprobar, igual que "no se resuelve".

Las funciones llamadas también se compilan transitivamente, así que **una función que (aunque sea
indirectamente) llama a algo que no se puede compilar no se puede compilar**. El proceso no falla; se rechaza
con un error que lo dice. Todas las funciones incorporadas se pueden compilar, así que las únicas funciones que
se rechazan de este modo son las que llaman a las siguientes operaciones solo del intérprete:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Las que son solo del intérprete son `compile`/`compile-file`/`dump` y `trace`/`untrace`/`step`/`disassemble`
([Herramientas de la implementación](functions/system.md#5-herramientas-de-la-implementación-clhs-252)). Más que
cosas que no se pueden compilar, son operaciones del lado que compila (lo que escribe `dump` es el propio entorno
del intérprete, que un ejecutable AOT no tiene; lo que vigila `trace` y donde se detiene `step` son los caminos
de llamada del intérprete en marcha; y `disassemble` usa el propio compilador). `room`/`dribble`/`ed` no están
entre ellas y se pueden compilar con normalidad.

Lo que **sí** se puede compilar: E/S de streams y archivos, `random`, `gensym`, `symbol->string`/
`intern`, `parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, las
funciones trascendentes, las operaciones de bits, `catch`/`throw`/`unwind-protect`, los cuatro
`eq`/`eql`/`equal`/`equalp` (lo que permite compilar `case` para cualquier tipo), toda la familia de impresión
incluidos `print`/`println`/`format`/`pprint` y `pprint-logical-block`, `read` y `eval`. La biblioteca estándar
se distribuye ya compilada.

Un ejecutable AOT contiene solo las funciones que usa el programa. Un programa que no imprime no lleva motor de
formato, uno que no llama a `read` no lleva lector, y uno que no llama a `eval` no lleva comprobador ni
intérprete.

Desde la línea de órdenes, `typl -c src-path [-o out-path]` (`-c` también se puede escribir `--compile`) hace lo
mismo que `compile-file`. Sin `-o`, la salida es `src-path` sin la extensión `.typl`. Por defecto, la biblioteca
estática `libtypelisp_front.a` que se enlaza en los ejecutables es, para una compilación de release de `typl`,
la que `typl` lleva dentro, escrita en el primer enlazado en `$TYPELISP_HOME/lib/<ID de compilación>/` (o en
`~/.typelisp/lib/<ID de compilación>/` sin `TYPELISP_HOME`) y usada desde ahí; para una compilación de
depuración, la del lugar donde se compiló `typl`. `typl --remove-lib` borra lo que escribió ese `typl`. Con
`--others`, borra las de otros ID de compilación; con `--all`, las de todos los ID de compilación. Con
`typl --lib-dir DIR`, se usa la de `DIR` (tanto para `-c` como para `compile-file`), y si no está ahí, es un
error al arrancar.

### 10.1 Volcados

```lisp
(dump "session.typld")     ; escribir uno
```
```sh
typl --image session.typld prog.typl   # arrancar desde él
typl --image session.typld             # también el REPL
```

Un volcado contiene información de tipos y cuerpos compilados en un archivo. Lo que escribe `(dump path)` es lo
que cargó la sesión actual (la biblioteca estándar, o un volcado pasado con `--image`) más **lo que definió la
propia sesión**. Así que la salida es autocontenida, y `typl --image` levanta el mismo entorno. Lo que la sesión
compiló con `(compile f)` se escribe en su forma compilada.

Lo que se guarda son **definiciones, no historia**:

- Las expresiones de nivel superior de la sesión (`(println ...)`, etc.) no se incluyen. Sería un problema que
  se volvieran a ejecutar al cargar.
- Las variables globales vuelven con **el valor de su inicializador ejecutado de nuevo**, no con el valor del
  momento del volcado. Es una diferencia deliberada con el `save-lisp-and-die` de SBCL (que escribe el montículo
  tal cual), y esta elección hace desaparecer toda una familia de problemas: los "valores que no se pueden
  guardar", como streams abiertos, punteros a función de cierres y memoria externa.
- A diferencia de `save-lisp-and-die`, **el proceso no muere**, ya que escribir no daña la imagen.

Un volcado registra las versiones de la biblioteca estándar y del compilador de la implementación que lo
escribió. Cargarlo con un `typl` de otra versión es un error; nunca se acepta en silencio.

### 10.2 `eval` en ejecutables AOT

`eval` comprueba los tipos contra "el entorno global actual" y después evalúa
([Análisis y evaluación](functions/system.md#6-análisis-y-evaluación)). Ese entorno (las tablas de firmas, tipos y
macros que consulta el comprobador, y los cuerpos que puede ejecutar el intérprete) **no está en el código
máquina**. Una función compilada no es más que un símbolo colocado en una dirección; no tiene ni los tipos de sus
argumentos ni una tabla para buscar cuerpos por nombre.

Así que, solo para los programas que llaman a `eval`, `compile-file` **construye ese entorno en tiempo de
compilación y lo escribe en el ejecutable**. El formato es el mismo que el de un volcado, y contiene la parte de
la biblioteca estándar y la parte propia del programa. Al arrancar solo se restaura: no se vuelve a leer el
código fuente ni se vuelve a comprobar nada. A los programas que no llaman a `eval` no se les añade nada.

Consecuencias:

- **El arranque tarda más y el ejecutable es más grande**, ya que entran el código del comprobador y del
  intérprete y una instantánea del entorno. El montículo también se hace algo más grande.
- **Las formas que se pasan a eval se interpretan.** Incluso cuando la forma pasada a eval llama a las propias
  funciones del programa, lo que se ejecuta es el cuerpo interpretable que guarda la instantánea. El resultado
  es el mismo; solo cambia la velocidad.

El almacenamiento de las variables globales se **comparte** con el código compilado (las mismas ranuras). El
inicializador de un `defvar` lo ejecuta una vez la inicialización compilada, y la restauración se lo salta, así
que un inicializador con efectos secundarios no se ejecuta dos veces.

`compile-file` también lee la biblioteca estándar (e incrusta sus cuerpos en el ejecutable), así que las
funciones de la biblioteca estándar como `abs`/`gcd`, y `(impl print-object ...)`, se pueden usar con AOT.

`compile-file` también acepta `use` (y `import`/`shadowing-import`). El `(use m)` del archivo de entrada busca
archivos con las mismas reglas que `typl file.typl`, y los archivos de dependencia encontrados también se
compilan y se enlazan en el ejecutable: una organización en la que `main.typl` lee `http.typl` mediante
`(use http)` se puede compilar con AOT tal cual. Las propias definiciones del archivo de entrada también van al
módulo con el nombre del archivo, como con `typl file.typl` (`point` en `p.typl` es `p::point`). Así que la
representación impresa de los valores (`#<p::point x: 1 y: 2>`) es la misma se ejecute como se ejecute.

## 11. Macros de lectura (readtable)

Lo que **hace el lector cuando se encuentra con cierto carácter** se puede sustituir desde el programa
(CLHS 23.1).

```lisp
(set-macro-character c f)             ; f lee el carácter c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f lee la secuencia de dos caracteres d s
(get-dispatch-macro-character d s)    ; Option<f>
```

El tipo de `f` es `(fn (string-input-stream char) Option<Sexpr>)`. El primer argumento es **un stream sobre el
texto aún no leído**, y el segundo es **el carácter que lo disparó** (el segundo carácter para un despacho). El
valor de retorno se convierte en los datos leídos en ese punto. El stream es un tipo concreto en lugar de
`:dyn PeekInput` porque el lector siempre pasa esta única clase: `read-sexpr` / `read-char` / `peek-char` /
`unread-char` / `read-delimited-list` reciben todas `(:where (PeekInput S))`, así que todas funcionan sobre el
tipo concreto tal cual.

```lisp
(use std::reader::read-sexpr)
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => se lee como (not (equal 1 2)), es decir, true
```

El lector **mira los caracteres de macro antes que la sintaxis incorporada**, así que también puede tomar el
control de `(` y `'`. Los subcaracteres de `#` registrados así tienen prioridad sobre los `#b`/`#x`/`#.`
incorporados. Un carácter distinto de `#` se convierte en carácter de despacho en el acto al pasarlo a
`set-dispatch-macro-character`: **no** hay equivalente al `make-dispatch-macro-character` de CL. El registro ya
hace su trabajo, así que un paso aparte no tendría nada que hacer.

**Cuándo surten efecto** depende del camino de lectura, igual que con `#.` (capítulo 1):

- El REPL y `(load ...)` ejecutan una forma cada vez, así que **las funciones definidas en formas anteriores**
  se pueden registrar tal cual.
- Los archivos de módulo se comprueban como una unidad y se ejecutan después, así que **solo las llamadas a
  `set-macro-character` / `set-dispatch-macro-character` se ejecutan de inmediato** (el papel del
  `(eval-when (:compile-toplevel) ...)` de CL). Como se ejecutan de inmediato, **la función que se pasa debe
  existir ya en ese punto**. Un `defun` del mismo archivo aún no se ha ejecutado, así que escribe una
  `lambda`, o usa la biblioteca estándar o algo que ya se haya ejecutado. Solo se tienen en cuenta las llamadas
  de nivel superior; no mira dentro de `progn` ni de `let`.

Los `read` / `read-from-string` incorporados también consultan la readtable (como en CL).

**Lo que no hay**: `*readtable*` y `copy-readtable`, ni `readtable-case`. Los dos primeros porque una readtable
**no es un valor**: un valor tendría que ser "algo que se puede entregar a un lector", pero el lector que lee el
código fuente está fuera del programa, sin ningún sitio al que entregársela. `readtable-case` porque el
capítulo 1 decide que el lector de este lenguaje siempre pasa a minúsculas (el `:downcase` de CL).


## 12. Concurrencia (tareas)

**Una tarea es un hilo ligero** (en términos de Go, lo que inicia una sentencia `go`) y se ejecuta de forma
cooperativa (no hay expropiación). El cambio no pasa por el núcleo, y el estado de ejecución vive en el montículo
y no en una pila de máquina, así que crear tareas en gran número es barato.

**Las tareas se ejecutan a la vez en varios hilos de SO** (paralelismo multinúcleo). El número de hilos es la
variable de entorno `TYPELISP_THREADS` (el total, incluido el hilo que ejecuta `main`; por defecto es el
paralelismo de la máquina). En `typl`, **solo las tareas compiladas** se ejecutan en otros hilos, y las tareas
interpretadas se ejecutan en el hilo del intérprete (12.7). Los datos compartidos pasan por `Mutex<T>` o
`Chan<T>`; las lecturas y escrituras simultáneas que no lo hacen están indefinidas, como en Go (12.7).

Del vocabulario, **solo `task` / `thread` / `select` son formas especiales**; el resto son funciones, métodos y
macros normales ([Tareas y canales](functions/concurrency.md)).

### 12.1 `task` — iniciar una tarea

```lisp
(task (f arg...))                   ; devuelve Task<T>, donde T es el tipo de retorno de f
```

**Solo acepta la forma de una llamada.** `f` y cada `arg` se evalúan donde se escribe el `task`, en el orden
escrito, y en la tarea nueva solo ocurre **la llamada**. Es la misma regla que el `go f(x)` de Go, y también la
razón por la que acepta una forma de llamada en lugar de un thunk: un thunk capturaría sus argumentos sin
evaluarlos.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i se evalúa en el acto cada vez; sin trampa de captura

(task ((lambda () ()                ; para ejecutar un cuerpo arbitrario, llama a una lambda
         (println "start")
         (send ch 1))))
```

Las formas especiales (`if` / `let` / `progn` …) no se pueden escribir directamente bajo `task`.

**Por qué no puede ser una función**: escribir `(spawn (lambda () T body...))` exigiría escribir `T`, ya que
`lambda` exige una anotación de tipo de retorno, y una macro no conoce el tipo de retorno de `(f a b)`. Solo lo
conoce el comprobador.

### 12.2 `thread` — iniciar una tarea en un hilo de SO dedicado

```lisp
(thread (f arg...))                 ; devuelve Thread<T>, donde T es el tipo de retorno de f
(join th)                           ; espera a que termine y devuelve su valor (cualquier número de veces)
```

La forma y las reglas de evaluación son las mismas que en `task` (solo acepta una forma de llamada, y `f` y `arg`
se evalúan donde se escribe). La diferencia es dónde se ejecuta: **inicia un hilo de SO dedicado a esa tarea y
se ejecuta solo en él**. No se multiplexa con otras tareas, así que llamar dentro a una función de C bloqueante
(`defffi`) detiene solo ese hilo, y las demás tareas avanzan. Dentro de él se pueden usar `task`, `send`, `recv`
y el resto tal cual.

- `Thread<T>` es la contrapartida de `Task<T>`. Como `wait`, `join` detiene **la tarea que llama**, y el valor
  se guarda en caché. Cuando termina la tarea, el hilo también termina.
- Las reglas de panic son las mismas que para `task` (cae el proceso entero). Cuando `main` retorna, el
  proceso termina.
- Para escribirlo como función, usa `(Thread::spawn (lambda () T body...))` (el `std::thread::spawn` de Rust).
  También se puede pasar una función con nombre.
- **En un hilo dedicado solo se ejecuta código compilado.** Cuando `typl` evalúa `(thread (f ...))` o
  `Thread::spawn` mientras interpreta, compila en el acto la función que se va a ejecutar (y lo que llama) antes
  de ejecutarla. Lo que no se puede compilar (una `lambda` que hace referencia a variables locales de fuera,
  construir una estructura, etc.) es, antes de iniciar el hilo, un panic tratado igual que un `(panic ...)`. Una
  `lambda` que hace referencia a variables locales se puede pasar si se crea dentro de una función compilada.

### 12.3 `select` — esperar varias operaciones de canal a la vez

```lisp
(select
  ((v (recv ch1)) body...)          ; una rama de recepción. v se liga a un Option<T>
  ((send ch2 x) body...)            ; una rama de envío
  (:else body...))                   ; opcional. **si se escribe, va al final**
```

- **Con `:else`, no bloquea** (el `default` de Go). Sin él, espera hasta que una sea posible.
- **Si varias son posibles a la vez, se elige una al azar** (en el orden escrito, las ramas posteriores
  pasarían hambre).
- La `v` de una rama de recepción es un **`Option<T>`**. Un canal cerrado es "una respuesta", no un motivo para
  saltarse la rama, así que haz `match` sobre ella dentro de la rama.
- El tipo es **la unión de los tipos de los cuerpos de todas las ramas** (la misma regla que para las ramas de
  `match`).
- `(select)` con cero ramas es un error de tipos (no se adopta el `select{}` de Go, que bloquea para siempre).
  Un `select` con solo `:else` también lo es, ya que equivale a escribir el cuerpo directamente.

**Las expresiones de canal y los valores a enviar se evalúan una vez cada uno, de izquierda a derecha, sea cual
sea la rama elegida** (la misma disciplina que tiene `case` con sus claves).

```lisp
(use std::sync::after)
(select                             ; recibir con tiempo de espera máximo
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([un canal que entrega tras un tiempo](functions/concurrency.md#5-after--un-canal-que-entrega-tras-un-tiempo))
es "un canal que entrega un valor pasados `sec` segundos", y corresponde al `time.After` de Go.

### 12.4 Interacción con otras funciones

| Función | Cómo se relaciona con las tareas |
|---|---|
| `catch` / `throw` | **No cruzan las fronteras de las tareas.** Un `throw` que intenta salir del cuerpo de una tarea es un panic |
| `unwind-protect` | La limpieza se ejecuta cuando una tarea termina con normalidad. **No se ejecuta cuando el proceso termina porque terminó la tarea principal** |
| `block` / `return-from` | Léxicos, así que no cruzan las fronteras de `lambda` |
| `panic` | Como en Go, cae el proceso entero. `wait` no observa un panic como valor |
| `dlet` | **No es un enlace por tarea.** Sigue "tomando prestada y devolviendo una global", así que las tareas interfieren entre sí |
| Salida estándar | La comparten todas las tareas. La salida de un `println` nunca se mezcla con otras a mitad de línea |
| `compile` / `eval` | Sin restricciones. `(compile f)` dentro de una tarea funciona |

### 12.5 Dónde cambian las tareas

La planificación es cooperativa, así que **las tareas solo cambian donde lo escribes**: `(yield)`,
`(sleep ...)`, `(wait ...)`, **las operaciones de canal que tienen que esperar** (`send`/`recv`/`select`), y
**las operaciones de socket que tienen que esperar** (`accept` / `tcp-connect` (incluida la resolución de
nombres) / leer y escribir sockets / `recv-from`; [Red](functions/network.md)). Todos los sockets son no
bloqueantes: si uno no está listo, solo se detiene esa tarea, y se reanuda cuando el SO dice que está listo, con
la misma forma que el netpoller de Go. Solo cuando no puede ejecutarse ninguna tarea espera la implementación al
SO hasta el plazo de `sleep` más cercano.

Las operaciones de canal que pueden responder en el acto (un `send` con sitio en el búfer, un `recv` con un
valor esperando, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **no consumen el turno**. Esto significa que
una lectura no te interrumpe inesperadamente, y se trata de forma distinta a `(sleep 0.0)`, que es el "ceder
durante 0 segundos" de CL.

**No hay expropiación.** Un bucle cerrado que no llama a nada deja sin turno a las demás tareas. Sin embargo, los
bucles compilados ceden el control al planificador periódicamente, así que un bucle cerrado compilado no las
deja sin turno.

### 12.6 Código compilado y tareas

El código compilado también puede suspender tareas. Lo mismo vale para los ejecutables creados con
`compile-file`: `main` se ejecuta como la tarea principal del planificador, y `task`, `sleep`, `wait`, los canales
y las esperas de sockets funcionan todos con el mismo significado que en `typl`. Cuando `main` retorna, el
proceso termina y las tareas restantes se cortan (como en Go). El intérprete nunca se mete en el ejecutable por
el planificador.

La única excepción es "dentro de un callback del FFI de C", donde las operaciones que **tendrían que esperar** son
errores (más amables que un interbloqueo silencioso): mientras C llama a una función pasada con `defffi`, la pila
de C está encima, y no hay forma de suspender la tarea y reanudarla después.

Los siguientes lugares también son funciones llamadas en mitad de una tarea, y sin embargo no pueden
suspenderse: los métodos `print-object`, `~/name/` en `format`, las macros de lectura, el interior de `eval` y
los inicializadores de `defvar` en los ejecutables AOT. Aquí, **las operaciones que responden sin esperar
pasan** (`(recv ch)` con un valor en el búfer, `read-line` sobre un socket con datos ya recibidos,
`(task ...)`, `(yield)`, etc.), y **las operaciones que de verdad tendrían que esperar son errores** (no se
detiene el proceso en el acto, sino que es un panic como `` `recv` cannot block: ... ``, tratado igual que un
`(panic ...)`).

### 12.7 Diferencias con Go

- **En `typl`, solo las tareas compiladas salen a otros hilos.** El estado del intérprete no se puede compartir
  entre hilos, así que las tareas de un `task` interpretado se ejecutan en el hilo del intérprete. También una
  tarea compilada **se traslada al hilo del intérprete y se queda ahí** (no vuelve) en el momento en que llama a
  un valor de función interpretado, llama a un método `:dyn` que nadie ha compilado, o llama a
  `eval`/`macroexpand`/`read`. Si un cálculo largo toca código interpretado aunque sea una vez por el camino, el
  resto se ejecuta en el hilo del intérprete.
- **En `typl`, los trabajadores viven solo durante una evaluación de nivel superior.** Mientras el REPL espera
  una entrada, y entre formas de nivel superior, los demás hilos no hacen avanzar las tareas (las tareas
  restantes continúan donde lo dejaron en la siguiente evaluación). Al final de una evaluación, espera a que cada
  hilo termine su paso actual, así que si una función de C (`defffi`) sigue bloqueando dentro de un `thread`, la
  evaluación no termina hasta que retorna.
- **Imprimir en los workers**: los métodos `print-object` interpretados, y las funciones y métodos
  interpretados que llama `~/name/`, no pueden ejecutarse en otros hilos, así que imprimir esos valores en
  otro hilo es un panic tratado igual que un `(panic ...)` (`(compile T::print-object)`, o imprime desde la
  tarea principal).
- **Las carreras de datos están indefinidas** (la misma postura que Go). El resultado de que varias tareas
  cambien el mismo valor sin pasar por `Mutex<T>` / `Chan<T>` no está garantizado.
- **`task` devuelve un valor.** A diferencia de la sentencia `go` de Go, devuelve un `Task<T>`, y `(wait t)`
  obtiene el resultado.
- **No hay canales nil.** El modismo de fan-in de Go (poner a `nil` un canal cerrado para quitarlo de las ramas
  de `select`) no se puede escribir, así que inicia una tarea por entrada y reúnelas con un `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--esperar-n-finalizaciones)). Es también la forma recomendada
  en Go, pero es **la primera diferencia con la que se topa quien viene de Go**.

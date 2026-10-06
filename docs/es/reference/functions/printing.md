<!-- translated-from: docs/ja/reference/functions/printing.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Impresión

`print`/`println`/`format`, las impresoras de un argumento, el pretty printer, `print-object` y las variables
que controlan la impresión. La lista de directivas de formato está en [format.md](format.md). Leer de
streams y escribir en ellos está en [Streams y archivos](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` son todas **formas especiales que interpretan directivas de formato (las
directivas del `format` de CL)**. El primer argumento (el segundo en `format`) es la **cadena de control**, y
cada directiva consume por turno los argumentos variádicos que siguen.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expande la cadena de control y la escribe en la salida estándar sin salto de línea |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Lo mismo, con un salto de línea al final |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | El `format` de CL. Devuelve la cadena expandida. Si `dest` es `true` (el `t` de CL), también se escribe en la salida estándar; si es `false` (el `nil` de CL), no se escribe y solo se devuelve |
| `format` (a un stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Si `dest` no es un `bool`, es el destino stream de CL. La cadena expandida se escribe en ese stream. El valor de retorno es `()` (el `nil` de CL), y no se devuelve ninguna cadena |

El tipo de `dest` divide el significado en dos (cuál se aplica se decide estáticamente). La forma con
stream se puede escribir igual con un tipo de stream concreto, un `:dyn CharOutput` o una variable de tipo
ligada por `(where (CharOutput S))`. Un `dest` que no es ni `bool` ni stream es un error de tipos.

**La cadena de control debe ser un literal** (la misma restricción que el `format!` de Rust). Las directivas
que contiene deciden cuántos argumentos se toman y de qué tipos, así que una cadena construida en tiempo de
ejecución no se puede leer al comprobar. Como debe ser un literal, **el número y los tipos de los argumentos
se comprueban al comprobar**: `(println "~d" "x")` y `(println "~a ~a" 1)` son errores al comprobar. Una
directiva mal escrita, un `~(` sin cerrar y un `~/name/` que ningún argumento puede responder también son
errores al comprobar. Las reglas de comprobación están en [format.md](format.md#1-cómo-escribir-directivas).
Para imprimir una cadena que construyes, créala con `(format false ...)` e imprímela con `(println "~a" s)`.

Los argumentos variádicos se envuelven en `Sexpr` con sus propios tipos antes de pasarse: `i32`/`f64`/
`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, así como los `defstruct`/`defenum`/`Vector<T>`/
`HashTable<K,V>` definidos por el usuario y similares, se pueden pasar todos tal cual
(`(println "~a" my-struct)` simplemente funciona).

Ejecutar un script con `typl file.typl` **no imprime los valores de las expresiones de nivel superior**, así
que un programa escribe en la salida estándar llamando a estas. `print`/`println`/`format` envían su
salida en cada llamada (para que una petición de entrada sea visible antes de leer la entrada estándar,
incluso a través de una tubería).

**`Option<Sexpr>` se imprime de forma transparente.** El tipo de los datos de expresiones S es
`Option<Sexpr>`, así que el envoltorio `(some x)` no aparece en la salida y el contenido se imprime tal cual.
La lista vacía se imprime como `()`. Los demás `Option<T>` se imprimen como `(some ...)` / `none`. Lo mismo
vale para los campos `Option<T>` dentro de estructuras, enumeraciones y `Vector`. Un
`Result<Option<Sexpr>,…>` de `(eval ...)` se imprime como `(ok 42)`, o `(ok ())` para `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; obtener solo la cadena, sin imprimir
  (println "~a" s))                   ; => id=42
```

## 2. Impresoras de un argumento

Las impresoras de CLHS 22.1.3. En lugar de expandir un formato, imprimen un único valor tal cual. El stream
se puede omitir (por defecto es `*standard-output*`).

| Nombre | Forma | Descripción |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Escribe en una forma que se puede volver a leer (lo mismo que `~s`) y devuelve `x` |
| `princ` | `(princ x [stream])` | Escribe en una forma para personas (lo mismo que `~a`) y devuelve `x` |
| `write` | `(write x [stream])` | `prin1` si `*print-escape*` es verdadero, `princ` si es falso. Devuelve `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Devuelve una cadena en lugar de escribir (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Lo mismo (`~a`). Lo mismo que `to-string` |
| `write-to-string` | `(write-to-string x)` | Lo mismo, siguiendo `*print-escape*` |

`print`/`println` **no** están entre ellas. Son abreviaturas de `format` que reciben una cadena de control,
un trabajo distinto del `print` de CL (salto de línea, después `prin1`, después un espacio), así que cada
una conserva su propio nombre. En consecuencia, **el `print` de un argumento de CL no tiene escritura en
este lenguaje**: escribe `prin1`.

Son macros, porque los argumentos variádicos de `format` no aceptan variables de tipo y el tipo tiene que
conocerse en el punto de llamada.

## 3. La entrada estándar y los streams estándar

**Leer la entrada estándar** no se hace con funciones dedicadas sino con los métodos de `CharInput` sobre el
stream estándar `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([métodos de stream](streams-files.md#2-métodos)). La salida estándar y la
salida de error estándar tienen igualmente `*standard-output*` / `*error-output*`, y se puede escribir en
ellas como en `(write-line *standard-output* s)` (`print`/`println`/`format` son atajos para cuando
necesitas expandir un formato, y siempre escriben en la salida estándar).

## 4. El pretty printer

Corresponde al Lisp Pretty Printer de CL (CLHS 22.2). **Divide la salida que no cabe en el ancho de línea,
siguiendo los bloques lógicos y los saltos de línea condicionales.**

### 4.1 Variables de control

Variables globales que se pueden asignar. Una vez hecho `setf`, afectan a toda la impresión posterior. Para
cambiar una temporalmente, usa `dlet` (6.3).

| Variable | Tipo | Valor por defecto | Significado |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Si es verdadero, `~a`/`~s`/`~w` y las directivas pretty toman el camino de impresión bonita |
| `*print-right-margin*` | `int` | `80` | El margen derecho (en columnas). 0 significa "sin margen, no dividir nunca". Un valor negativo es un error de impresión |
| `*print-miser-width*` | `int` | `0` | El ancho a partir del cual empieza el estilo miser. 0 corresponde al `nil` de CL (estilo miser desactivado). Un valor negativo es un error de impresión |

La familia `pprint` y `pprint-logical-block` siempre hacen impresión bonita sea cual sea `*print-pretty*`
(siguiendo la definición del `pprint` de CL).

### 4.2 Disposiciones listas para usar (formas especiales)

Como `print`, son formas especiales, así que el argumento puede ser de cualquier tipo.

| Nombre | Forma | Descripción |
|---|---|---|
| `pprint` | `(pprint x)` | Imprime bonito con la disposición por defecto. Como en CL, **escribe primero un salto de línea** y ninguno al final |
| `pprint-fill` | `(pprint-fill x)` | Llena cada línea con todo lo que cabe. No escribe salto de línea |
| `pprint-linear` | `(pprint-linear x)` | Si no caben todos los elementos en una línea, **un elemento por línea**. No escribe salto de línea |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Una tabla con columnas de `colinc` de ancho (16 por defecto). No escribe salto de línea. Un `colinc` negativo es un error |

La disposición por defecto (`pprint`, y `~a` con `*print-pretty*`) sigue el `*print-pprint-dispatch*` por
defecto de CL: abrevia `(quote x)` como `'x`, y da formato a las formas de código como
`defun`/`let`/`if`/`lambda` como "la cabeza y el número prescrito de argumentos en la primera línea, y el
resto del cuerpo sangrado dos columnas, una forma por línea". Las demás listas se llenan.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Construir bloques lógicos por tu cuenta

| Nombre | Forma | Descripción |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Una forma especial que abre un bloque lógico. `obj` es la lista que recorre `pprint-pop` (`()` si no se recorre ninguna). `:prefix` y `:per-line-prefix` se excluyen mutuamente (como en CL) |
| `pprint-newline` | `(pprint-newline kind)` | Un salto de línea condicional. `kind` es `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Sangría. `kind` es `:block` (desde el inicio del bloque) / `:current` (desde la columna actual) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Una tabulación. `kind` es `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` y `colinc` no son negativos (error si lo son) |
| `pprint-pop` | `(pprint-pop)` | Toma el siguiente elemento de la lista del bloque (`()` si se ha agotado) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Si la lista se ha agotado |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Si se ha agotado, hace `break` del `loop` envolvente (una macro) |

Los bloques lógicos no reciben un argumento stream: **un bloque lógico abierto es un estado implícito**. El
`pprint-logical-block` más externo lo inicia, y cuando se cierra, todo se formatea y se escribe de una vez
en la salida estándar. Mientras está abierto, la salida de `print`/`println`/`(format true ...)`/`pprint`
va toda a ese bloque, así que **escribes el contenido con `print` normal y marcas solo los lugares donde
dividir con `pprint-newline` y compañía**, lo que hace que el código se vea casi igual que en CL.

En CL, `pprint-exit-if-list-exhausted` es una salida no local de `pprint-logical-block`; aquí es **un
`break` del `loop` envolvente** (`pprint-logical-block` no establece un `block`). El modismo de CL siempre lo
pone dentro de un `loop` de todos modos, así que se lee igual.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Reglas de los saltos de línea condicionales (CLHS `pprint-newline`):

- `:mandatory` divide siempre.
- `:linear` divide si el bloque lógico envolvente no cabe en una línea. La decisión es por bloque, así que
  **todos los saltos `:linear` de un bloque dividen a la vez** (es el "todo en una línea o un elemento por
  línea" de `pprint-linear`).
- `:fill` divide si (a) la siguiente sección no cabe en el resto de la línea, (b) la sección anterior no
  cupo en una línea, o (c) en estilo miser, el bloque no cabe en una línea.
- `:miser` funciona como `:linear` solo en estilo miser (cuando el bloque empieza a menos de
  `*print-miser-width*` del margen derecho).

## 5. `print-object` (representación impresa por tipo)

Escribir `impl print-object <tipo>` hace que `print`/`println`/`format`/`pprint` impriman los valores de
ese tipo con esa implementación, **incluso cuando están anidados dentro de listas**. Corresponde a la
función genérica `print-object` de CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argumento | Significado |
|---|---|
| `self` | El valor que se imprime |
| `escape` | El `*print-escape*` de CL. `true` para `~s`/`prin1`/`pprint` (una forma que se puede volver a leer), `false` para `~a`/`princ` (para personas). Una implementación a la que no le importe puede ignorarlo |

La `string` devuelta va directamente a la salida. Los tipos sin `impl` se imprimen con la representación
incorporada (de la forma `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   también funciona anidado
```

También se combina con el pretty printer (capítulo 4). Si `*print-pretty*` es verdadero, una lista que
contiene las cadenas que devolvió la implementación se divide en el margen derecho.

Las representaciones impresas de los tipos de la biblioteca estándar. Los tipos que también existen en CL
se imprimen igual que en SBCL. Cuando el REPL muestra un resultado, usa la misma representación que `~s`.

| Tipo | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#<vector<int> 1 2 3>` | Igual (elementos con `~a`) |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Igual |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (el número es un número de serie interno) | Igual |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Un entero (el valor de `get-universal-time` / `get-internal-real-time` de CL) | Igual |
| Tipos de error (`ParseIntError`, `SimpleError`, etc.) | `#<simpleerror "boom">` | Solo el mensaje (`boom`) |
| `complex` | `#C(1.0 2.0)` | Igual |
| `Array<T>` | `#2A((0 0) (0 0))` | Igual |
| Streams | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Igual |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Igual |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` al final durante el horario de verano) | Igual |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Igual |
| Tipos `defstruct` | `#<point x: 1 y: 2>` (nombres y valores de los campos) | Igual (campos con `~a`) |

Reglas:

- **El registro es estático.** Un `impl` se comprueba como una definición de método normal, así que un
  nombre de tipo mal escrito o una firma incorrecta es un error de compilación.
- **También funciona para tipos genéricos.** `(impl print-object box<T> (where (print-object T)) ...)` va a
  un cuerpo distinto para cada argumento de tipo: un valor recuerda su tipo incluidos sus argumentos de tipo
  (`box<i32>`). Los tipos genéricos incorporados como `Vector<T>` funcionan igual.
- **La elección se hace al imprimir.** Qué directiva consume qué argumento depende del contenido en tiempo
  de ejecución de la cadena de control, así que la distinción entre `~a` y `~s` (es decir, `escape`) solo se
  conoce en el momento de imprimir. Es lo mismo que en CLOS, donde los métodos `print-object` "se definen por
  clase y se eligen al imprimir".
- **La reentrada vuelve a la representación incorporada.** Si una implementación se imprime a sí misma con
  `(format false "~a" self)`, recursaría para siempre, así que cuando un valor que se está imprimiendo
  vuelve a aparecer, se usa la representación incorporada. Esto mira la identidad del valor, no un límite de
  profundidad, así que no estorba al imprimir legítimamente estructuras autorreferentes anidadas.
- **Todos los tipos escalares implementan este trait.** Es **para que se pueda usar como restricción**: los
  argumentos variádicos de `format` no pueden tomar variables de tipo, así que esta restricción es la única
  forma en que el código genérico puede decir "los valores de un tipo desconocido se pueden representar" (la
  misma forma que el `T: Display` de Rust). El `print-object` de `Array<T>` es un ejemplo.
- **Con argumentos de tipo que no cumplen la restricción, se usa en silencio la representación
  incorporada.** `(impl print-object Array<T> (where (print-object T)))` se aplica a `Array<i32>`, pero no a
  un `Array` cuyos elementos son un `defstruct` sin `print-object`. No tendría sentido que el mero hecho de
  crear un arreglo fuera un error, así que no es un error.
- El otro mecanismo de CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (un registro en tiempo de
  ejecución indexado por especificadores de tipo), **no se adopta**. Sus registros no se comprueban, lo que no
  encaja con un lenguaje de tipado estático.

## 6. Controlar cuánto se imprime

### 6.1 Profundidad, longitud y compartición

Las variables de control de CLHS 22.1.1 que deciden "cuánto de un valor se imprime". Como las tres de 4.1,
son globales que se pueden asignar, y se aplican a todo `print`/`println`/`format`/`pprint`, sea
`*print-pretty*` verdadero o no.

| Variable | Tipo | Valor por defecto | Significado |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Los objetos anidados a esta profundidad o más se sustituyen por `#`. El objeto que se imprime está en la profundidad 0. 0 significa ilimitado |
| `*print-length*` | `int` | `0` | Imprime los elementos de las listas (y los campos de los valores `defstruct`/`defenum`) hasta esta cantidad y sustituye el resto por `...`. 0 significa ilimitado |
| `*print-circle*` | `bool` | `false` | Si es verdadero, el valor se examina antes de imprimir y **los objetos que aparecen dos o más veces reciben etiquetas**. La primera aparición es `#n=…` y las posteriores `#n#` |

CL usa `nil` para "ilimitado", pero este lenguaje no tiene `nil`, así que, como con `*print-right-margin*`,
**0 significa ilimitado**. Los valores negativos no tienen sentido y son errores de impresión. Los valores
por defecto son todos "sin límite / sin etiquetas", igual que los valores iniciales de CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Las estructuras circulares solo se pueden imprimir cuando `*print-circle*` es verdadero.** Si imprimes un
valor que se apunta a sí mismo mientras es falso (el valor por defecto), la impresora sigue el ciclo
indefinidamente y el proceso falla. CL es igual (CLHS deja indefinida la impresión de estructuras
circulares cuando `*print-circle*` es falso).

Un ciclo solo se puede crear "apuntando con `setf` un campo de `defstruct` a sí mismo" (las celdas `Sexpr`
no se pueden cambiar tras crearse, así que una lista como `'(1 2 3)` nunca puede ser circular):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a apunta al propio a
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Las etiquetas **vuelven a empezar desde 1 en cada cosa que se imprime** (como en CL). Incluso sin ciclo, si
el mismo objeto aparece dos veces recibe `#1=`/`#1#`, conservando en la salida la información de que "estos
dos son el mismo objeto", como especifica CL:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Un valor sin compartición **no muestra ninguna etiqueta**, así que dejar esta variable a verdadero no cambia
la salida del código cotidiano.

### 6.2 Base, mayúsculas y legibilidad

| Variable | Tipo | Valor por defecto | Significado |
|---|---|---|---|
| `*print-base*` | `int` | `10` | La base para imprimir enteros (de ancho fijo e `int`). Fuera de 2 a 36 es un **error de impresión** (CL también especifica el rango) |
| `*print-radix*` | `bool` | `false` | Si es verdadero, añade una marca de base: `#b`/`#o`/`#x`, `#NNr` para otras bases, y un `.` final para la base 10. La marca va **antes** del signo (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Las mayúsculas de los nombres de símbolo: `:upcase` / `:downcase` / `:capitalize` (las mismas escrituras que CL). Cualquier otro símbolo es un error de impresión |
| `*print-readably*` | `bool` | `false` | Si es verdadero, imprime en una forma que se puede volver a leer. Fuerza el escape y desactiva los recortes de `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | El número de líneas que puede usar el pretty printer. El exceso se corta, con `..` al final como en CL. 0 significa ilimitado. Un valor negativo es un error de impresión |
| `*print-escape*` | `bool` | `true` | Si `write`/`write-to-string` hacen `prin1` o `princ`. **Solo esas dos lo leen** |
| `*print-array*` | `bool` | `true` | Si `Array<T>` muestra su contenido. Si es verdadero, la sintaxis de arreglos de CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); si es falso, solo la forma, `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Las marcas que añade `*print-radix*` las puede volver a leer el lector (la notación de base de la
[Referencia de sintaxis](../syntax.md#1-elementos-léxicos)).

**Por qué el valor por defecto de `*print-case*` difiere del de CL**: el valor por defecto de CL es
`:upcase` porque el lector de CL guarda los nombres de símbolo en mayúsculas, es decir, significa "tal como
se guardaron". Este lector los guarda en minúsculas, así que el valor por defecto con el mismo significado
es `:downcase`.

**La mitad que falta de `*print-readably*`**: CL señala `print-not-readable` para los valores que no se
pueden volver a leer, pero este lenguaje no tiene condición que señalar, ni forma de decidir la legibilidad
de los tipos de usuario, que `print-object` puede imprimir de cualquier manera. Solo están el escape
forzado y la anulación de los recortes.

**Por qué solo `write` lee `*print-escape*`**: como especifica CLHS, `~s`/`prin1`/`pprint` lo ligan a
verdadero, y `~a`/`princ` a falso, cada uno solo mientras dura su propia llamada. Así que los únicos lectores
que lo ven sin ligar son `write`/`write-to-string`. Una implementación de `print-object` debería leer su
propio argumento `escape` en lugar de esta global: ese argumento lleva el valor que eligió la directiva.

**Lo que tiene CL y este lenguaje no**: `*print-gensym*` (no hay símbolos sin internar).

### 6.3 Sustituciones temporales

CL los liga con `let`, pero `let` en este lenguaje enlaza léxicamente, así que usa `dlet`
([Otros](system.md#10-otros)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; los límites se aplican solo a esta impresión
(with-standard-io-syntax (println "~a" x))   ; imprimir con todo de vuelta en los valores estándar
```

`with-standard-io-syntax` ejecuta su cuerpo con todas las variables de control de la impresora en sus
valores estándar y `*read-eval*` a `true`.

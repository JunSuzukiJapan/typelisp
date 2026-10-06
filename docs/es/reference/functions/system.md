<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Tiempo, entorno e implementación

Funciones de tiempo, consultas sobre el entorno de ejecución, herramientas de la implementación, análisis y
evaluación de texto, docstrings y macros.

## 1. Tiempo

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Dos campos: `day` (días desde 1900-01-01) y `second` (el segundo dentro de ese día, 0..86399) |
| `internal-time` | — | `defstruct` | Dos campos: `second` y `microsecond` (dentro de ese segundo, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | El tiempo desde la época de CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Tiempo transcurrido relativo al proceso |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | El **tiempo de CPU** que ha usado este proceso (usuario más sistema) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Como número de segundos. La forma de informar la diferencia entre dos lecturas |
| `internal-time-units-per-second` | — | `int` | `1000000` (microsegundos), la unidad del campo `microsecond`. Como en CL, el valor lo elige la implementación |
| `time` | `(time form)` | Macro | Ejecuta `form`, imprime el tiempo real y el tiempo de CPU en una línea cada uno, y devuelve el valor de `form` tal cual |

El tiempo real y el tiempo de CPU dicen cosas distintas. En un trabajo que espera sobre todo por E/S, los dos
difieren mucho, y esa diferencia es justo lo que quieres saber, así que `time` muestra los dos.

`sleep`, que detiene una tarea, está en [Tareas y canales](concurrency.md#3-yield--sleep--ceder-el-turno).

## 2. Decodificar y codificar fechas

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Nueve campos**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Los nueve valores de retorno de CL como una estructura (no hay valores múltiples) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Tiempo universal a componentes de calendario. `zone` son horas al oeste de Greenwich (la misma dirección que CL). **Si se omite, es la hora local** (como en CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Lo inverso. Sin `zone`, los argumentos se leen como **hora local** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | El momento actual, decodificado en hora local |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | El desfase de la hora local al oeste de Greenwich, en **segundos**, en ese tiempo universal |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Si el horario de verano estaba en vigor en ese tiempo universal |

Como en CL, en `day-of-week` **0 es el lunes y 6 el domingo**.

**Sin `zone`, se usa la hora local**, como en CL. El desfase local se pregunta al SO, así que el resultado
depende de dónde esté la máquina. **Dar una zona explícita lo hace determinista**, y `0` es UTC.

La unidad de `zone` es, como en CL, "horas al oeste de Greenwich", así que UTC+9 se lee como `-9`. Sin
embargo, **el argumento es un entero y el campo `zone` del resultado es un `f64`**. Los desfases reales no
siempre son horas enteras (India es +5:30, Nepal +5:45), y redondear el valor informado mentiría en
silencio. Una zona que escribes a mano es un número entero de horas, así que el argumento es `int`.

Cuando se da `zone`, `daylight-p` es `false` y `zone` es exactamente el valor dado, como especifica CL
(*If a time-zone is supplied, daylight saving time information is ignored*).

Una hora local que cae dentro de una transición de horario de verano no es única de entrada, y CL no dice
cuál tomar. `encode-universal-time` devuelve una de las dos respuestas para esa hora.

## 3. El entorno de ejecución

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | La línea de órdenes. **El elemento 0 es el nombre del programa** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Una variable de entorno. `none` si no está definida o no es UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. La base de `user-homedir-pathname` ([Nombres de ruta](streams-files.md#92-funciones)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | La versión de la implementación |
| `machine-type` | `(machine-type)` | `()→string` | La arquitectura de CPU (`x86_64` / `aarch64` …). El valor del **destino de compilación** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | El nombre de host |
| `machine-version` | `(machine-version)` | `()→Option<string>` | El nombre del hardware **en ejecución ahora** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` donde no se puede determinar |
| `software-type` | `(software-type)` | `()→string` | El SO (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | La versión del SO (`uname -r`, por ejemplo `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Un nombre corto del sitio de instalación. **Siempre `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Igualmente, un nombre largo. **Siempre `none`** |

Las que devuelven `Option` son elementos para los que CL permite `NIL` (*or nil if no such name can be
determined*). POSIX no tiene dónde registrar nombres de sitio, así que siempre son `none`; SBCL devuelve lo
mismo. Fíjate en la diferencia entre `machine-type` y `machine-version`: el primero es la arquitectura para
la que se **compiló** este binario, el segundo es el chip que lo **ejecuta** ahora.

El elemento 0 de `command-line-args` es la ruta del script para `typl script.typl a b`, y el propio
ejecutable para un ejecutable AOT lanzado como `./prog a b`. **Cualquiera de las dos formas de ejecutarlo
lee los mismos argumentos en los mismos índices** (`typl` quita su propio nombre y opciones como
`--heap-cells` antes de pasarlos).

## 4. Preguntar al usuario

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Acepta un único `y` / `n`. Vuelve a preguntar hasta obtenerlo |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Hace que el usuario escriba entero `yes` / `no`. Para preguntas en las que un error sale caro |

Ambas leen de `*standard-input*`. Solo el final de la entrada detiene las repreguntas, y entonces el
resultado es `false`.

## 5. Herramientas de la implementación (CLHS 25.2)

La capa donde la implementación responde preguntas sobre sí misma. `heap-info` / `room` / `dribble` son
funciones normales; `trace` / `untrace` / `step` / `disassemble` / `ed` son **formas especiales**
(`trace` / `untrace` / `disassemble` / `ed` reciben el *nombre* de una definición, y `step` una *forma*, todo
sin evaluar).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | El estado actual del montículo como estructura. Los mismos números que imprime `room` |
| `room` | `(room &optional verbose)` | `(bool)→()` | Informa de `heap-info` en `*standard-output*`. `(room true)` da más detalle |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Empieza a registrar la salida de la sesión en `path` / deja de registrar cuando se llama sin argumento |
| `trace` | `(trace name...)` | `Sexpr` | Informa en `*trace-output*` de las llamadas a las definiciones nombradas. Devuelve la lista de nombres trazados ahora |
| `untrace` | `(untrace name...)` | `Sexpr` | Deja de informar. **Sin argumentos, los quita todos** |
| `step` | `(step form)` | El tipo de `form` | Evalúa `form`, parándose en cada llamada para preguntar |
| `disassemble` | `(disassemble name [llvm])` | `()` | Imprime en qué se convierte esa definición. El código máquina del anfitrión por defecto, LLVM IR con `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Arranca `$VISUAL` / `$EDITOR`. Con un nombre, abre la línea donde está escrita esa definición |

`trace`/`untrace`/`step`/`disassemble` son solo del intérprete, y las funciones que las llaman no se pueden
compilar ([capítulo 10 de la Referencia de sintaxis](../syntax.md#10-compilación)).

### 5.1 Campos de `heap-info`

| Campo | Tipo | Contenido |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Toda la arena de cons y su desglose. Siempre `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Las cuentas actuales de las otras tres clases de objetos del montículo |
| `gc-count` | `int` | El número de recolecciones desde que arrancó la implementación |
| `growable` | `bool` | Si la arena aún puede crecer |

Todos los campos son `int` (salvo `growable`). El límite de crecimiento (consulta la descripción de
`typl --heap-cells`) no se informa, porque lo que quiere saber quien lee es si aún puede crecer
(`growable`).

### 5.2 Lo que `trace` / `step` pueden ver y lo que no

- **También son visibles las definiciones con cuerpos compilados, desde puntos de llamada que se están
  interpretando.**
- **Los puntos de llamada *dentro* del código compilado no son visibles.** Trazar un nombre que tiene un
  cuerpo compilado añade una nota de una línea que lo dice. La misma limitación que describe SBCL para las
  llamadas locales.
- **Las llamadas a través de valores de cierre (`funcall`/`apply`) no son visibles.** Los cierres no tienen
  nombre.
- **Las definiciones genéricas no están incluidas.** En cada lugar de uso se crea una copia para cada tipo,
  así que no hay un único cuerpo que nombrar (el mismo motivo, y la misma redacción, que cuando `compile` se
  niega).

Las órdenes de `step` son `s` (entrar en esta llamada; una línea vacía hace lo mismo), `n` (saltar esta
llamada), `c` (dejar de preguntar a partir de aquí) y `q` (abortar). **Si la entrada estándar no es un
terminal, `step` simplemente evalúa `form`**: un comportamiento degenerado que CLHS permite explícitamente,
para que los scripts y las pruebas no se cuelguen ante una pregunta que nadie puede responder.

El `$VISUAL` / `$EDITOR` de `ed` se divide por los espacios en blanco, así que `EDITOR="code -w"` funciona.
Si no está definido ninguno, el resultado es `Err`: no adivina `vi`. El número de línea se pasa primero, con
la forma `+N`.

`dribble` registra las tres vías por las que la salida de la sesión sale del proceso: lo que escriben
`print`/`println`/`format`, lo que se escribe en streams conectados a la salida estándar, y las líneas
tecleadas en el REPL junto con los valores que el REPL imprime de vuelta.

## 6. Análisis y evaluación

Todas estas manejan texto y datos del tiempo de ejecución (que el propio programa no controla), así que, si
fallan, devuelven el `Err` de un `Result` en lugar de provocar un panic. Los tipos de error son tipos
concretos por operación ([Tipos de error](option-result.md#3-tipos-de-error-y-el-trait-error)).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | El `parse-integer` de CL. Se salta los espacios en blanco iniciales y finales (el mismo conjunto que `trim`), lee como mucho un signo `+`/`-` y después dígitos en base `radix` (10 por defecto, de 2 a 36; los dígitos por encima de 10 en mayúsculas o minúsculas). No hay límite de dígitos (`int`). Cualquier otro carácter sobrante da `Err`. Con `:junk-allowed true`, se detiene en el primer no dígito e ignora el resto, pero da `Err` si no hay ni un dígito (corresponde al `nil` de CL). No devuelve el segundo valor de CL (la posición donde terminó la lectura). Un `radix` fuera de rango provoca un panic (un error de quien llama, no del texto) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Un número de coma flotante. También acepta `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Lee un `Sexpr` de `s` (con el mismo lector que lee el código fuente). Paréntesis desequilibrados, cadenas sin terminar y similares dan `Err`. Leer de un stream es `read-sexpr` ([Streams](streams-files.md#6-funciones-genéricas-y-operaciones-con-archivos)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` más **la posición donde terminó la lectura**. `(car r)` es el valor y `(cdr r)` la posición del siguiente carácter a leer. `start` es 0 por defecto |
| `read-from-string-preserving-whitespace` | Igual que arriba | Igual que arriba | Lo mismo, pero no consume el espacio en blanco que terminó el dato. La diferencia se ve en la posición devuelta |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Comprueba los tipos de `form` en tiempo de ejecución y lo evalúa. Sigue el `eval` de CL |

CL devuelve **dos valores** (el valor y la posición) de `read-from-string`, pero este lenguaje no tiene
valores múltiples, así que devuelve un `cons-cell`. Tener la posición hace que leer una cadena dato a dato
sea un bucle en lugar de volver a recorrerla:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

La diferencia que introduce `preserving-whitespace` es **un carácter de espacio en blanco**: el `read` de CL
consume el espacio que terminó el dato, y `read-preserving-whitespace` lo deja. `(read-from-string "12 34")`
devuelve la posición 3, y la versión que lo conserva devuelve 2.

La sintaxis numérica que acepta el lector está en el [capítulo 1 de la Referencia de sintaxis](../syntax.md#1-elementos-léxicos).
Lo que imprime `*print-radix*` ([Impresión](printing.md#62-base-mayúsculas-y-legibilidad)) se puede volver a
leer tal cual. No existe el `*read-base*` de CL.

### 6.1 Qué significa `eval`

Sigue el `eval` de CLHS: evalúa en **el entorno global actual** (funciones, variables, tipos y macros
globales, incluidas las definiciones añadidas en tiempo de ejecución) y en **el entorno léxico nulo** (los
enlaces locales de los `let`/`lambda` de quien llama no son visibles). Se pueden evaluar tanto expresiones
como definiciones (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`), y las definiciones se registran en el
entorno global de inmediato y de forma permanente.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; la x global es visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; devuelve el nombre definido
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; la definición recién hecha es visible
```

- **Valor de retorno**: para una expresión, el resultado como `Option<Sexpr>`; para una definición, el
  símbolo del nombre definido (como en CL). Para usar el resultado, desarma el `Sexpr` con `match`
  (`(int n)`/`(str s)`/…).
- **Diferencias debidas a los tipos estáticos (importante)**: CL devuelve el valor real del resultado, pero
  en este lenguaje el tipo de retorno solo puede ser uniformemente `Result<Option<Sexpr>,EvalError>`. Además,
  **el código escrito estáticamente no puede referirse por adelantado a nombres que `eval` define en tiempo
  de ejecución**: un `(sq 9)` escrito directamente en un archivo se comprueba antes de que se ejecute el
  `eval` que define `sq`, y está "sin definir". Sin embargo, **los `eval` posteriores sí lo ven** (su
  comprobación de tipos se ejecuta en tiempo de ejecución, después de la definición). El REPL comprueba y
  ejecuta una línea cada vez, así que un nombre definido con `eval` se puede llamar directamente desde la
  línea siguiente.
- **Errores**: los errores de tipos y de sintaxis devuelven `Err` (no provocan un panic). Los **panics en
  tiempo de ejecución** del código evaluado (división por cero, etc.) se propagan igual que lo harían desde
  código escrito directamente. Se ejecuta la limpieza de cualquier `unwind-protect` intermedio
  ([capítulo 8 de la Referencia de sintaxis](../syntax.md#8-salidas-no-locales-catch--throw--unwind-protect)).
- **Espacio de nombres**: cuando lo ejecuta `typl file.typl` y dentro de un ejecutable AOT, `eval` evalúa en
  el espacio de nombres del módulo del script (las globales propias del script son visibles). El REPL evalúa
  en el espacio de nombres raíz.
- **Compilación**: tanto `read` como `eval` se pueden compilar. Cómo se tratan en los ejecutables AOT, y sus
  consecuencias (las formas que se pasan a eval se interpretan), está en
  [Referencia de sintaxis 10.2](../syntax.md#102-eval-en-ejecutables-aot).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (incluidos los de dentro de `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/
`defenum`/`deftype`/`deftrait` pueden llevar docstrings. La posición sigue la regla de CL para cada uno:

| Forma | Posición de la docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | Al principio del cuerpo (tras el tipo de retorno y la cláusula `where`). Solo cuando la sigue al menos una forma del cuerpo; una cadena sola sigue siendo el valor de retorno |
| `defvar` / `defconstant` | **Después** del valor inicial: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Justo después** del nombre, antes de los campos/variantes |
| `deftype` | **Justo después** del nombre, antes del tipo: `(deftype meters "doc" i32)` |
| `deftrait` | Justo después de la lista de supertraits, antes de los elementos. Una para todo el trait. **Los métodos con implementación por defecto** pueden poner su propia docstring justo antes de su cuerpo |

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `documentation` | `(documentation name)` | (forma especial; `name` es un símbolo simple o `Type::method`)→`Option<string>` | Devuelve la docstring de `name` |

Como `quote`/`compile`, `documentation` es una forma especial (lee `name` como un nombre sin evaluar). A
diferencia del `(documentation 'name 'function)` de CL, no recibe argumento de tipo; en su lugar resuelve un
nombre simple en el orden **variable → función → tipo → trait → macro** (la misma prioridad que para un
identificador simple evaluado como expresión). La forma `Type::method` busca la docstring de un método
asociado o estático.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**El valor se decide al comprobar**: si el nombre no se resuelve a ninguna definición, es un error al
comprobar (como referirse a una variable no definida). Si se resuelve pero no hay docstring, el resultado es
`Option::none`.

**No incluido**:

- `(setf documentation)` (cambiar una docstring en tiempo de ejecución) no existe.
- No se admiten nombres libres calificados con módulo (`mod::name`; `Type::method` sí se admite).
- Una declaración de método en un `deftrait` **sin cuerpo** no puede tener docstring. Un literal de cadena
  final sería él mismo el cuerpo (el valor de retorno) de una implementación por defecto, así que no hay
  forma de distinguir los dos.

La información al pasar el ratón del servidor de lenguaje (`typl-lsp`) también muestra docstrings.

## 8. Macros

Cómo definir macros está en [Referencia de sintaxis 3.14](../syntax.md#314-defmacro--definición-de-macros).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Un símbolo nuevo. Su nombre es `" <prefix><n>"`, donde `n` es `*gensym-counter*`. Un espacio inicial no se puede escribir en el código fuente, así que los enlaces generados nunca chocan con nombres escritos |
| `*gensym-counter*` | Variable | `int` | El número que usará `gensym` a continuación. Como en CL, se puede leer y fijar |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expande un paso una llamada a macro. `none` significa "no es una llamada a macro" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Repite hasta que deja de ser una macro |

`macroexpand-1` devuelve un `Option`. CL informa de "si se expandió" como segundo valor de retorno, pero no
hay valores múltiples, así que `none` hace ese papel. **Una macro que se expande en una llamada a sí misma
nunca se puede confundir con algo que no es una macro.** Un paso de expansión es el mismo que usa el
comprobador de tipos, así que lo que ve el programa y lo que vio el comprobador nunca divergen.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none se muestra como la lista vacía (Option<Sexpr> es transparente)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Lo que tiene CL y este lenguaje no: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` siempre
coinciden, así que no hay distinción que elegir), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (símbolos sin internar; los enlaces se buscan por nombre, así que no
habría nada que ganar).

## 9. Enlaces de macros locales (`macrolet` / `symbol-macrolet`)

Ambas son formas especiales que enlazan léxicamente **nombres que no son valores**. No queda nada en tiempo
de ejecución: lo que se compila es la forma expandida del cuerpo.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Un enlace de `macrolet` oculta una macro global del mismo nombre **solo durante el cuerpo**. La lista lambda
  es la misma que la de `defmacro` (`&optional`/`&rest`/`&key`).
- **Los hermanos de un mismo `macrolet` no se ven entre sí desde sus *cuerpos*** (como en CL; es la diferencia
  con `labels`). Las expansiones se comprueban en el lugar de uso, así que un `earlier` que se expande en
  `(later ...)` funciona: ambos son visibles en ese lugar.
- Un nombre de `symbol-macrolet` entra en el entorno como un enlace normal. Así que un `let` interior oculta
  el mismo nombre, y una variable exterior queda oculta: las reglas de CL salen tal cual.
- **`setf` escribe en la expansión.** `(setf head 42)` es `(setf (get v 0) 42)`.
- Las expansiones se comprueban en **el entorno del lugar de uso** (no en el del lugar del enlace).

## 10. Otros

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic si es falso. Sin mensaje, `assertion failed: <la prueba tal como se escribió>` (es una macro, así que puede nombrar la propia expresión). Los reinicios de CL no existen en este lenguaje |
| `warn` | `(warn control args...)` | `(string,...)→()` | Escribe una línea con el prefijo `WARNING: ` en `*error-output*` y **continúa**. Una forma de informar de algo sin devolver un `Result` y sin terminar el programa |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Sustituye globales solo durante `body` y las restaura al salir. CL escribe esto como `let`, pero `let` en este lenguaje siempre enlaza léxicamente, de ahí el nombre aparte (el mismo papel que la macro homónima de Emacs Lisp). Las restaura se salga como se salga del cuerpo: terminación normal, `throw`, `panic`, `break`/`return`. **No es un enlace por tarea** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Ejecuta `body` con todas las variables de control de la impresora en sus valores estándar y `*read-eval*` a `true` ([Impresión](printing.md#6-controlar-cuánto-se-imprime)) |
| `exit` | `(exit code)` | `int→!` | Termina el proceso |
| `dump` | `(dump path)` | `string→bool` | Escribe el entorno actual (información de tipos más cuerpos compilados) en un archivo. `typl --image <path>` vuelve a arrancar desde él. Solo del intérprete ([Referencia de sintaxis 10.1](../syntax.md#101-volcados)) |

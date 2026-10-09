<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Streams y archivos

Traits y métodos de stream, tipos de stream concretos, operaciones con archivos y nombres de ruta. Los
sockets de red también son streams, y se tratan en [Red](network.md).

## 1. La jerarquía de traits

Lo que CL expresa con una jerarquía de clases se expresa aquí con una **jerarquía de traits**. Tanto la
dirección (entrada / salida) como el tipo de los elementos se deciden **estáticamente**, así que no hace falta
preguntar en tiempo de ejecución "¿se puede leer este stream?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; entrada de caracteres
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; salida de caracteres
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; entrada que puede devolver un carácter
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; entrada de bytes
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; salida de bytes
```

Una función que lee caracteres acepta cualquier tipo de stream, incorporado o definido por el usuario, si
recibe `(where (CharInput S))` o `:dyn CharInput`.

## 2. Métodos

Todos los métodos de `CharInput` tienen una implementación por defecto. Una implementación solo escribe
`read-item`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | El siguiente elemento. `none` al final. **El único método que hay que implementar** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | El siguiente carácter |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Hasta el siguiente salto de línea (el salto se consume y se quita). También se devuelve una última línea que no termina en salto de línea |
| `read-all` | `(read-all s)` | `(S)→string` | Todo lo que queda |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Solo un carácter que ya está a mano. `none` en lugar de esperar |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Añade hasta `n` caracteres a `v` y devuelve cuántos se leyeron realmente. Menos de `n` solo al final |

`listen` está en `InputStream` (el padre de `CharInput`):

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Si la siguiente lectura se puede responder sin esperar. El valor por defecto es `false`, **el lado que nunca es mentira**: `true` sería una suposición, y una suposición equivocada haría que `read-char-no-hang` bloqueara. Todos los streams incorporados lo sobrescriben. **En los streams definidos por el usuario que no lo sobrescriben, `read-char-no-hang` siempre devuelve `none`** |

`PeekInput` (que hereda de `CharInput`) añade **devolver un carácter**. Solo el propio stream tiene dónde
guardar el carácter devuelto, así que esto no puede tener implementación por defecto y es un trait aparte.
`file-stream`/`string-input-stream`/`standard-stream` lo implementan, y cualquier otro stream lo obtiene al
envolverlo con `make-peek-stream` (capítulo 4).

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Hace que la siguiente lectura devuelva `c`. **El único método que hay que implementar**. Como en CL, solo se garantiza un carácter |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Mira el siguiente carácter sin consumirlo |

Del mismo modo, para `CharOutput` una implementación solo escribe `write-item`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Escribe un elemento. **El único método que hay que implementar** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Escribe un carácter |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Escribe una cadena |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Una cadena y un salto de línea |
| `terpri` | `(terpri s)` | `(S)→()` | Un salto de línea (el nombre de CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Un salto de línea salvo al principio de una línea |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Si el siguiente carácter escrito empezará una línea. El valor por defecto es `false` (así que `fresh-line` escribe el salto: ante la duda, escribir es el lado seguro). Todos los streams incorporados lo sobrescriben |
| `finish-output` | `(finish-output s)` | `(S)→()` | Vacía el búfer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Escribe todos los caracteres de `v` en orden |

`at-line-start` recuerda **solo lo que se escribió a través de ese stream**. `print`/`println`/
`(format true ...)` escriben en la salida estándar sin pasar por `*standard-output*`, así que si mezclas los
dos, `(fresh-line *standard-output*)` no sabe de los saltos de línea que escribió `println`. Quédate con uno.

`Stream` es común a todos los streams:

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Si sigue abierto |
| `close` | `(close s)` | `(S)→()` | Lo cierra. **El GC no cierra los streams**, así que hazlo explícitamente (o con `with-open-file`) |

## 3. Tipos de stream concretos

| Tipo | Cómo crear uno | Traits implementados |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` es una de las tres constantes `direction-input` / `direction-output` / `direction-append`.
`open-file` devuelve `Err(FileError)` si el archivo no se puede abrir (que falte un archivo es un resultado
normal, no un panic). El nombre de archivo puede ser una cadena o un `pathname` (`Pathish` en el
capítulo 9).

`(get-output-stream-string s)` devuelve lo que se ha escrito en un `string-output-stream` y lo vacía. Como en
CL, se puede sacar incluso después de `close`.

**La E/S de bytes** usa `ByteInput`/`ByteOutput`. Fijan el `Item` de `InputStream`/`OutputStream` a `int`,
igual que `CharInput`/`CharOutput` lo fijan a `char`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | El siguiente byte. `none` al final del archivo |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Escribe un byte. Error fuera de 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | La versión de caracteres, en bytes |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Igual que arriba |

CL decide el tipo de los elementos en **la llamada**, como en `(open name :element-type '(unsigned-byte 8))`,
pero aquí el tipo de los elementos es **el tipo** del stream, así que lo que cambia es la función que lo
abre. Leer bytes de un stream de caracteres es un error de tipos (`string-input-stream` no implementa
`ByteInput`). Leer un byte justo después de devolver un carácter con `unread-char` también es un error.

## 4. Streams compuestos

Todos son `defstruct` de la biblioteca estándar y se pueden anidar.

| Nombre | Forma | Descripción |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Escribe en todos los de un `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Lee de `in` y escribe en `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Lee de `in` y además escribe en `out` los caracteres leídos |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Lee un `Vector<:dyn CharInput>` uno tras otro |
| `make-peek-stream` | `(make-peek-stream in)` | Añade la devolución de un carácter a cualquier `:dyn CharInput`, convirtiéndolo en un `PeekInput` (para `read-sexpr`) |

## 5. Macros

| Nombre | Forma | Descripción |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Abrir, ejecutar el cuerpo, cerrar. `Result<valor del cuerpo, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Lee de una cadena |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Devuelve lo que se escribió |

## 6. Funciones genéricas y operaciones con archivos

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Transfiere todo |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Todas las líneas que quedan |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Lee un `Sexpr` (el `read` de CL). `Ok(eof)` al final de la entrada, `Ok(datum d)` cuando lee uno, `Err` si no son datos. **Consume el carácter de espacio en blanco** que terminó el dato (como en CL). `ReadOutcome` no es un `Option<Sexpr>` para que leer la lista vacía `()` y el final de la entrada no sean el mismo valor |
| `read-sexpr-preserving-whitespace` | Igual que arriba | Igual que arriba | Lo mismo, pero deja el espacio en blanco (el `read-preserving-whitespace` de CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Lee hasta `ch` y forma una lista. `ch` se consume. `Err` si la entrada se agota |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Escribe una línea cada vez |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Todo el contenido |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Todas las líneas |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Lo escribe |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Si existe |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Borrar, renombrar (los argumentos son `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | La ruta absoluta con los enlaces simbólicos y `.`/`..` resueltos. `Err` si no existe |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | La hora de la última modificación. Es **tiempo universal**, así que `decode-universal-time` ([Tiempo](system.md#2-decodificar-y-codificar-fechas)) lo puede leer |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | El nombre de usuario del propietario. `Err` si el archivo no existe, `Ok(none)` si el uid del propietario no tiene entrada en la base de datos de contraseñas: se mantienen separados los dos casos que distingue CL |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Si es un directorio. **También `false` si no existe**; usa `probe-file` para distinguir los dos casos |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Enumera el contenido por truename (la ruta absoluta con los enlaces simbólicos resueltos, como con `truename`). Se omiten los enlaces simbólicos cuyo destino falta. Se omiten `.`/`..`. El orden es el que dé el SO |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Lo crea junto con sus padres. Tiene éxito si ya existe |

Todo argumento que nombra un archivo **puede ser una cadena o un `pathname`**. Es el mismo trato que los
designadores de nombres de ruta de CL, resuelto mediante el trait `Pathish` en lugar de una comprobación de
tipo en tiempo de ejecución (capítulo 9).

El carácter de terminación de `read-delimited-list` **también termina los tokens**. Solo surte efecto en la
profundidad 0: en `(1 2]` el `]` se lee como parte del propio texto de la lista y se informa como una lista
rota. No hay equivalente al tercer argumento `recursive-p` de CL.

## 7. Hacer de tu propio tipo un stream

Escribe un `write-item` y las implementaciones por defecto traen el resto. También puede ir dentro de
streams compuestos.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; todos los métodos restantes son los por defecto

(write-line (counter::new 0) "four")   ; write-line, terpri y fresh-line funcionan todos
```

La entrada funciona igual: solo escribes `read-item`. Incluso un tipo sin devolución propia se puede leer con
`read` una vez envuelto, como en `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nombre | Llamada | Tipo | Descripción |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` lee el carácter `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Devuelve lo que está registrado |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` lee la secuencia de dos caracteres `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Igual que arriba |

`F` es `(fn (string-input-stream char) Option<Sexpr>)`. Cómo usarlos, cuándo surten efecto y en qué se
diferencian de CL está en la [Referencia de sintaxis](../syntax.md#11-macros-de-lectura-readtable).

## 9. Nombres de ruta `pathname`

Un nombre de archivo dividido en partes. Contiene los componentes de directorio separados por `/`, el
nombre, el tipo (extensión) y si empieza en la raíz.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   dividido en el último punto
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 El trait designador de rutas `Pathish`

Donde CL acepta un designador de nombre de ruta (una cadena o un nombre de ruta), este lenguaje acepta un
`Pathish`. Lo implementan tanto `string` como `pathname`, y **todas las operaciones con archivos lo reciben de
forma genérica**, así que `(open-input "a.txt")` y `(open-input p)` son llamadas normales (no hay comprobación
de tipo en tiempo de ejecución). El `namestring` de una cadena se devuelve a sí misma, así que mientras pases
una cadena no se hace ningún análisis.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | La forma de cadena. Hay que implementarlo |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Convierte a `pathname` (la función `pathname` de CL, renombrada porque chocaría con el nombre del tipo). Hay que implementarlo |

### 9.2 Funciones

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Divide una cadena en partes. Un `/` final (o un nombre vacío) significa "sin nombre", es decir, un directorio |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Construye uno solo con los componentes dados (todos `&key`). Un nombre o tipo omitido queda "ausente" y es algo que `merge-pathnames` rellena |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Los componentes de directorio, el más externo primero |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | El nombre sin el tipo. `none` para un directorio |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Lo que va tras el último punto. Un punto inicial no cuenta (todo `.gitignore` es el nombre) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Si empieza en la raíz |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | El directorio personal. `none` si no hay `$HOME` (CL también permite `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | La parte hasta el último `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Solo la parte `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Rellena los componentes que faltan en `p` a partir de `default`. Un `p` relativo va bajo el directorio de `default`; un `p` absoluto conserva su propio directorio |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | La forma relativa a `default`. Todo `p` si no está bajo la base |

Los argumentos de tipo llevan todos `(where (Pathish P))`.

## 10. Diferencias con CL

- **Una jerarquía de traits, no de clases.** No existen `input-stream-p` / `output-stream-p`: el tipo lleva la
  dirección, así que no es una pregunta que hacer en tiempo de ejecución.
- **`read` tiene nombres distintos para la versión de cadena y la de stream.** `(read "...")` (corresponde al
  primer valor del `read-from-string` de CL; si también necesitas la posición donde terminó la lectura, usa
  `read-from-string`) y `(read-sexpr s)` (el `read` de CL). Una llamada se resuelve a un único tipo de
  receptor, así que el mismo nombre no se puede sobrecargar.
- **La devolución de caracteres es un trait aparte** (`PeekInput`), así que los tipos que solo necesitan
  `read-char` no están obligados a implementar `unread-char`.
- **El cierre es explícito.** El GC no cierra los streams (el GC se ejecuta en momentos impredecibles, así que
  dejárselo haría impredecible también el momento del cierre). Usar `with-open-file` es la forma segura.
- **Los nombres de ruta no tienen componentes de host, dispositivo ni versión.** No hay nombres de ruta con
  comodines ni nombres de ruta lógicos (`logical-pathname`). El separador es siempre `/`.
- **La función `pathname` es `to-pathname`**, porque los tipos, los traits y las funciones comparten un
  espacio de nombres.
- **No hay coincidencia por comodines**, así que `directory` es una función que "enumera el contenido de ese
  directorio" y nada más. El `directory` de CL busca coincidencias con un patrón de nombre de ruta.

<!-- translated-from: docs/ja/reference/functions/format.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Directivas de formato

Las directivas que se escriben en las cadenas de control de `print`/`println`/`format`. Cubren casi todas
las directivas del `format` de CL. Las funciones en sí se describen en
[Impresión](printing.md#1-print--println--format).

## 1. Cómo escribir directivas

Cada directiva es `~`, después unos **parámetros prefijos** opcionales (separados por comas: un entero /
`'c` (un carácter) / `v` (tomado del siguiente argumento) / `#` (el número de argumentos restantes)),
después los **modificadores** opcionales `:` y `@`, y después el carácter de la directiva, en ese orden.
Los caracteres de directiva no distinguen mayúsculas.

La cadena de control debe ser un literal ([Impresión](printing.md#1-print--println--format)). Además, al
comprobar se verifica lo siguiente.

- **El número y los tipos de los argumentos.** Para cada directiva que consume un argumento: si queda un
  argumento y si se acepta su tipo (las notas sobre "argumento" de las tablas de abajo). Donde el camino
  depende de valores en tiempo de ejecución, como moverse con `~*`, qué cláusula de `~[` se toma, si se
  dispara `~^` o cuántas veces se repite `~@{`, se comprueban **todos los caminos**. Los argumentos
  sobrantes no son problema (como en CL).
- **Parámetros y modificadores.** Un modificador no aceptado, demasiados parámetros y valores fuera de rango
  (un ancho negativo, una base distinta de 2 a 36, un entero donde se espera un carácter, etc.) son errores.
  Nunca se ignoran ni se redondean en silencio.

Los **elementos** de un argumento lista (`~{`, `~:{`, `~<...~:>`) son `Sexpr`, y ni su número ni el tipo de
cada elemento se pueden saber a partir de los tipos. Las exigencias sobre los elementos (un entero para
`~d`, etc.) y los elementos que faltan se comprueban cuando llegan los valores, y son errores en tiempo de
ejecución (nunca se pasa a una representación distinta en su lugar).

No se adoptan las reglas permisivas de CL. Pasar un no entero a `~d` y que se imprima como `~a`, o que `~:[`
trate cualquier valor como booleano, no se reinterpretan así; son errores de tipos.

## 2. Salida (consume un argumento)

| Directiva | Parámetros / modificadores | Significado |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=justificar a la derecha | Estética (el `princ` de CL; cadenas sin comillas). El argumento puede ser de cualquier tipo |
| `~s` | Igual que arriba | Estándar (el `prin1` de CL; una forma que se puede volver a leer). El argumento puede ser de cualquier tipo |
| `~w` | — | El `write` de CL. Hace impresión bonita si `*print-pretty*` es verdadero; si no, lo mismo que `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=grupos de dígitos, `@`=signo siempre | Enteros en decimal/binario/octal/hexadecimal. El argumento es un entero |
| `~r` | `~radix,mincol,padchar,commachar,interval` (con base) o ninguno | Con base, esa base (2 a 36). Sin ella: `~r`=cardinal en inglés, `~:r`=ordinal en inglés, `~@r`=números romanos, `~:@r`=números romanos antiguos. El argumento es un entero |
| `~p` | `:`=retroceder uno, `@`=y/ies | Plurales (`~p`→"s", `~@p`→"y"/"ies"). El argumento es un entero |
| `~c` | `:`=nombre, `@`=sintaxis `#\` | Un carácter. El argumento es un `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=signo | Coma fija. El argumento es un número |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=signo | Notación exponencial. El argumento es un número. Los parámetros de dígitos del exponente, escala y overflowchar de CL no se admiten (darlos es un error) |
| `~g` | `@`=signo | Coma flotante general. El argumento es un número. No recibe parámetros |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notación monetaria. El argumento es un número |

## 3. Salida (no consume argumentos)

| Directiva | Significado |
|---|---|
| `~%` | Salto de línea (`~n%` para n de ellos) |
| `~&` | fresh-line (un salto de línea salvo al principio de una línea; `~n&`) |
| `~\|` | Salto de página (form feed) |
| `~~` | Un `~` literal (`~n~` para n de ellos) |
| `~t` | Tabulación (`~colnum,colincT`. Si ya está en la columna colnum o más allá, avanza un múltiplo de colinc; no se mueve si colinc es 0. `@`=relativa. `:`=una tabulación relativa al inicio del bloque lógico, que solo funciona con impresión bonita) |
| `~_` | Salto de línea condicional (pretty; simple=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Sangría (pretty; `~ni`=inicio del bloque + n / `~n:i`=columna actual + n) |
| `~<newline>` | Ignora el salto de línea (`:`=conservar el espacio en blanco, `@`=conservar el salto de línea) |

Como en CL, todas las directivas del pretty printer (`~_` `~i` `~:t` `~<...~:>`, y el camino de impresión
bonita de `~a`/`~s`/`~w`) no hacen nada cuando `*print-pretty*` es falso. Por defecto es falso.

## 4. Estructuras de control

| Directiva | Significado |
|---|---|
| `~(...~)` | Conversión de mayúsculas (`~(` minúsculas, `~:(` mayúscula inicial en cada palabra, `~@(` mayúscula inicial solo en la primera palabra, `~:@(` todo mayúsculas) |
| `~[...~;...~]` | Selección condicional (ramifica según un argumento entero. Con `~n[`, `~v[` o `~#[`, ramifica según ese valor y no toma argumento. `~:;`=la cláusula por defecto, solo como última cláusula). `~:[falso~;verdadero~]` ramifica según un argumento `bool` y tiene exactamente dos cláusulas |
| `~{...~}` | Iteración (recorre un argumento lista. `~:{`=por sublista, `~@{`=sobre los argumentos restantes, `~:@{`=sobre cada lista de los argumentos restantes, `~^`=salir, `~:}`=ejecutar una vez aunque esté vacía). Un cuerpo que no consume ningún argumento en una iteración es un error (nunca terminaría) |
| `~<...~;...~>` | Justificación (reparte segmentos en `~mincol` columnas. `:`/`@`=relleno en los extremos) |
| `~<...~;...~:>` | **Bloque lógico** (se cierra con `~:>`; es algo distinto de la justificación de arriba). El primer segmento es el prefijo y el último el sufijo (ambos solo cadenas literales). Con el separador `~@;`, el prefijo es un **prefijo por línea**. `~:<` pone por defecto el prefijo/sufijo a `(`/`)`. El argumento es una lista (`~@<` usa los argumentos restantes en su lugar) |
| `~*` | Saltar argumentos (`~n*`=avanzar n, `~:*`=retroceder, `~@*`=a una posición absoluta) |
| `~/name/` | Llamada a método (capítulo 5. Los indicadores `:`/`@` se pasan al método. No recibe parámetros) |

No se admiten las siguientes directivas de CL (son errores al comprobar).

- `~?` y `~@?`: reciben una cadena de control como argumento en tiempo de ejecución, así que no se pueden
  comprobar los argumentos que consumen sus directivas. Escribe esas directivas directamente en la cadena de
  control.
- `~@[...~]`: comprueba si un argumento no es nil, pero este lenguaje no tiene nil. Usa
  `~:[falso~;verdadero~]`, que ramifica según un `bool`.
- `~{~}` con cuerpo vacío: toma el cuerpo de un argumento en tiempo de ejecución. Escribe las directivas
  dentro de las llaves.

## 5. `~/name/`

**Una diferencia con CL: el nombre no se busca como función global sino como método del propio tipo del
argumento.** El método tiene la forma `((self Self) (colon bool) (at bool)) → string`, y los `:`/`@` de la
directiva se le pasan tal cual.

La forma de CL de buscarlo como función global no se puede implementar de forma segura en este lenguaje.
Incluso con una cadena de control literal, el tipo de los elementos de un argumento lista (dentro de `~{`)
no se conoce al comprobar, y buscar una función solo por su nombre podría llamar a una función pensada para
otro tipo. Elegir según el tipo del valor significa que el método se comprueba exactamente para ese tipo,
lo cual es seguro (el mismo mecanismo que `print-object`). También funciona para valores como
`string`/`bool`/`char`/`symbol`/listas. Solo para los enteros, cuyo ancho no se puede saber a partir del
valor, es un error **cuando más de un tipo entero define un método con ese nombre**.

No se sabe a qué argumento se aplica, pero sí qué métodos podría llamar. El comprobador reúne todos los
`~/name/` de la cadena de control literal y registra, entre los tipos de los argumentos de esa llamada,
los que tienen un método con la forma de arriba. Así que **si ninguno de los tipos de los argumentos tiene
el método, es un error al comprobar** (no en tiempo de ejecución), y también funciona en los ejecutables
AOT.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```

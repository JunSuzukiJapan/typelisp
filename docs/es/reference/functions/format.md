<!-- translated-from: docs/ja/reference/functions/format.md @ 7aae962c61e6f36448edc8bab6b37ff177aefdb3 -->
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

Los **elementos** de un argumento lista (`~{`, `~:{`, `~<...~:>`) son `Sexpr`; ni su número ni el tipo
de cada elemento se conocen por los tipos. Los requisitos sobre los elementos (un carácter para `~c`,
etc.) y la falta de elementos se comprueban al llegar los valores y son errores en tiempo de ejecución
(nunca se cambia a otra representación en su lugar).

`~d` `~b` `~o` `~x` y `~r` con base imprimen un no entero como lo haría `~a`, en decimal, como en CL
(`(format false "~5d" 1/2)` es `"  1/2"`). Las demás reglas permisivas de CL no se adoptan: que `~:[`
trate cualquier valor como booleano no se reinterpreta así; es un error de tipo.

## 2. Salida (consume un argumento)

| Directiva | Parámetros / modificadores | Significado |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=justificar a la derecha | Estética (el `princ` de CL; cadenas sin comillas). El argumento puede ser de cualquier tipo |
| `~s` | Igual que arriba | Estándar (el `prin1` de CL; una forma que se puede volver a leer). El argumento puede ser de cualquier tipo |
| `~w` | — | El `write` de CL. Hace impresión bonita si `*print-pretty*` es verdadero; si no, lo mismo que `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=grupos de dígitos, `@`=signo siempre | Enteros decimales/binarios/octales/hexadecimales. El argumento puede ser cualquiera (un no entero se imprime como con `~a`, en decimal) |
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
| `~/name/` | Llamada a una función o un método (capítulo 5; los indicadores `:`/`@` se pasan a lo llamado; no toma parámetros) |

No se admiten las siguientes directivas de CL (son errores al comprobar).

- `~?` y `~@?`: reciben una cadena de control como argumento en tiempo de ejecución, así que no se pueden
  comprobar los argumentos que consumen sus directivas. Escribe esas directivas directamente en la cadena de
  control.
- `~@[...~]`: comprueba si un argumento no es nil, pero este lenguaje no tiene nil. Usa
  `~:[falso~;verdadero~]`, que ramifica según un `bool`.
- `~{~}` con cuerpo vacío: toma el cuerpo de un argumento en tiempo de ejecución. Escribe las directivas
  dentro de las llaves.

## 5. `~/name/`

`~/name/` resuelve `name` con las mismas reglas que una llamada `(name arg colon at)` escrita en ese
lugar, y emite la cadena que devuelve la llamada. Consume un argumento. `colon` y `at` son `bool` que
indican si la directiva lleva `:` y `@`.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- La resolución del nombre es la misma que la de una llamada ([Referencia de sintaxis 3.10](../syntax.md#310-module--use--espacios-de-nombres)). Si
  el tipo del argumento tiene un método `name`, se llama a ese; si no, a la función visible `name`. Una
  función de un módulo se puede nombrar por su ruta (`~/m::shout/`).
- Lo que se puede llamar es una función o un método escrito en typelisp que recibe el argumento, `colon`
  y `at` tal cual y devuelve un `string`. No se pueden llamar funciones incorporadas.
- **A qué se llama se decide al comprobar.** El comprobador busca todos los argumentos que pueden llegar
  a ese `~/name/`, teniendo en cuenta los movimientos de `~*` y `~[`, y resuelve el nombre con el tipo de
  cada uno. Si alguno no se puede resolver es un error al comprobar, y también lo es el caso en que los
  argumentos que pueden llegar llamarían a dos definiciones distintas. En ejecución no se busca ningún
  nombre, así que funciona igual en los ejecutables AOT.
- **No se puede usar dentro de `~{`, `~:{`, `~:@{` ni `~<...~:>`** (error al comprobar). El tipo de los
  elementos de un argumento lista es `Sexpr`, que no sirve para decidir a qué llamar. `~@{` recorre los
  argumentos restantes de la llamada, cuyos tipos se conocen, así que ahí sí se puede usar.

La diferencia con CL: CL busca el nombre como función global. Aquí la resolución es la misma que la de una
llamada, así que un método del tipo del argumento se elige antes que una función del mismo nombre.

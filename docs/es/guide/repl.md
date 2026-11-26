<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# El REPL

Si se inicia `typl` sin argumentos, se entra en el REPL (modo interactivo). Este documento explica
lo que el REPL ofrece además de evaluar expresiones: sus comandos, la ayuda al escribir, el acceso
a resultados anteriores, cómo detener un cálculo y cómo investigar después de un error.

## Iniciar y salir

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Se sale con `:quit` (abreviado `:q`) o `Ctrl+D`. El historial de entrada se guarda en
`~/.typl_history`, de modo que la próxima vez se puede recuperar con `Ctrl+P` / `Ctrl+N` o
`Ctrl+R` (búsqueda en el historial).

Al iniciar, antes del primer prompt, se ejecuta lo siguiente en orden. Nada de ello muestra los
valores que produce.

1. El contenido de `~/.typlrc`, si existe. Con `--no-init` no se lee.
2. `--eval FORM` y `--load FILE` de la línea de comandos, en el orden escrito. Ambos pueden darse
   cualquier número de veces.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` y `--no-init` solo sirven para iniciar el REPL. Darlos al ejecutar un archivo
es un error.

El REPL empieza en el módulo `user`. Al pasar a otro módulo con `(in-module foo)`, el prompt
muestra ese módulo.

```
typl> (in-module foo)
typl user::foo>
```

## Comandos del REPL

Un `:nombre` escrito al principio de una línea es un comando del REPL. `:help` los enumera. Un
`:nombre` que no está en la lista no es un comando; se evalúa como una palabra clave normal.

| Comando | Significado |
|---|---|
| `:help` (`:h`, `:?`) | enumerar los comandos |
| `:type FORM` (`:t`) | mostrar el tipo de FORM sin evaluarlo |
| `:describe NAME` (`:d`) | mostrar el tipo de definición, su tipo, sus campos o variantes, sus métodos, los traits que implementa, dónde se definió y su cadena de documentación |
| `:apropos TEXT` | buscar en todos los módulos las definiciones cuyo nombre contiene TEXT (sin distinguir mayúsculas y minúsculas) |
| `:src NAME` | mostrar el código fuente de una definición |
| `:who-calls NAME` | mostrar las funciones y métodos que llaman a la función o método NAME |
| `:inspect FORM` (`:i`) | evaluar FORM y abrir su valor según su tipo |
| `:backtrace` (`:bt`) | mostrar las llamadas en curso cuando falló la línea anterior, de la más interna hacia fuera |
| `:locals [N]` | mostrar los argumentos de la llamada N de `:backtrace` (la 0 si se omite) |
| `:cd [DIR]` | cambiar la carpeta actual a DIR (a la carpeta personal si se omite) |
| `:pwd` | mostrar la carpeta actual |
| `:quit` (`:exit`, `:q`) | salir del REPL |

NAME puede ser un nombre como `inner` o una ruta como `std::seq::reverse` u `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Suma 1 a x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Suma 1 a x.
typl> :src inner
(defun inner ((x int) (s string)) int "Suma 1 a x." (+ x 1))
```

`:src` muestra una definición cargada de un archivo desde ese archivo, y una definición escrita en
el REPL desde el texto escrito. Las definiciones de la biblioteca estándar no tienen registrada su
ubicación en el código fuente, así que no se puede mostrar.

`:who-calls` busca en los cuerpos de las funciones de la sesión actual. En una función genérica
solo encuentra los usos que se han instanciado con tipos concretos.

`:cd` cambia la carpeta donde busca `(load ...)`, pero `use` sigue buscando archivos en la carpeta
donde se inició el REPL.

## Abrir un valor (`:inspect`)

`:inspect` abre un valor según su tipo. Un struct se abre en sus campos, un enum (incluido
`Option`) en los campos de su variante, un `Vector` en sus elementos y una `HashTable` en sus
claves y valores. Un número entra en esa parte, `u` sube un nivel y `q` termina.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Las listas y los `Sexpr` no se abren; se muestran tal cual.

## Ayuda al escribir

Al escribir en una terminal se dispone de lo siguiente.

- `Tab` completa nombres (definiciones y sintaxis como `if` y `defun`), rutas con `::`, comandos
  del REPL al principio de la línea y, dentro de una cadena, nombres de archivo.
- Tras escribir el nombre de una función seguido de un espacio, se muestra atenuado al final de la
  línea el tipo de esa función.
- Se resalta el paréntesis que corresponde al que está junto al cursor.
- Si se pulsa Intro antes de terminar una forma, la línea siguiente empieza sangrada según el
  número de paréntesis abiertos.

## Resultados anteriores (`:*`, `:**`, `:***`)

En el REPL, `:*` es el último valor mostrado, `:**` el anterior y `:***` el anterior a ese. Cada
uno tiene el tipo de su valor.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Una entrada que no mostró ningún valor (como una definición) o que falló no hace avanzar los
  resultados.
- Escribir `:**` o `:***` antes de que haya tantos resultados es un error.
- Un `':*` citado es una palabra clave normal.
- Un `:*` escrito en el cuerpo de una función sigue refiriéndose al resultado del momento en que se
  definió la función. El `*` de Common Lisp lee el valor en el momento de la llamada, pero
  typelisp lo fija al definir para que tenga un tipo.
- Fuera del REPL (al ejecutar un archivo, o en `~/.typlrc`, `--load` y `--eval`), `:*` es una
  palabra clave normal.

## Detener un cálculo (`Ctrl+C`)

Pulsar `Ctrl+C` durante una evaluación detiene ese cálculo y vuelve al prompt. Lo definido antes se
conserva. Funciona tanto si el cálculo es un bucle en el intérprete como un bucle en código
compilado o una espera en `sleep` o `wait`. Se ejecutan las tareas de limpieza de los
`unwind-protect` en curso.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` pulsado mientras se escribe solo descarta la línea que se está escribiendo.

## Investigar después de un error (`:backtrace`, `:locals`)

Cuando una evaluación se detiene con un error, `:backtrace` (`:bt`) muestra las llamadas a
funciones que estaban en curso en ese momento, de la más interna hacia fuera. `:locals N` muestra
los argumentos de la llamada N según sus tipos declarados.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- En una llamada hecha desde la última expresión de una función (una llamada de cola), no se
  muestra la función que llama. Gracias a ello, un bucle recursivo de cola usa el espacio de una
  sola llamada por muchas vueltas que dé.
- No se muestran las llamadas dentro de funciones compiladas ni las funciones anónimas (`lambda`).
- `:locals` muestra solo los argumentos, no las variables enlazadas con `let`.
- Al evaluar la línea siguiente se descarta el registro del error anterior.

<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Manejo de errores

El manejo de errores en typelisp divide los fallos en dos clases.

| Clase de fallo | Ejemplos | Cómo se expresa |
|---|---|---|
| Fallos que pueden ocurrir (recuperables) | Falta un archivo, la entrada no es un número | Devolver un `Result<T,E>` |
| Errores del programa (irrecuperables) | Un índice fuera de rango, `unwrap` de `none`, división por cero | Detenerse con `panic` |

Además están `catch` / `throw`, que salen de muchas llamadas a funciones de una vez, y
`unwind-protect`, que ejecuta una limpieza se salga como se salga de su cuerpo. Este capítulo supone que
has leído la sección de `Result` de [Fundamentos de tipos](types.md).

## 1. Devolver un `Result` y recibirlo con `match`

Esta es una función que lee un número de puerto de una cadena. Puede fallar de dos maneras: la entrada no
es un número, o está fuera de rango.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Quien llama separa el éxito del fallo con `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- El valor de una función que devuelve `Result` no se puede usar a menos que `match` trate el caso `err`.
  Olvidar tratar el fallo es un error de tipos.
- El error de `parse-int` es un valor de tipo `ParseIntError`. `(message e)` da su cadena de mensaje.

## 2. Pasar un fallo a quien llama

No hay una forma abreviada como el `?` de Rust. Al llamar sucesivamente a varias funciones que devuelven
`Result`, la parte de "devolver el fallo tal cual" se escribe con `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Cuando sabes que una operación no puede fallar, o en un script pequeño en el que detenerse ante un fallo
no importa, `unwrap` saca el contenido. Si el valor es un `err`, provoca un panic. Si basta con un valor
por defecto, usa `unwrap-or`.

## 3. Crear tu propio tipo de error

Expresar los errores como un tipo en lugar de una cadena permite que quien llama ramifique según la clase
de error. Un tipo de error es un `defenum` o `defstruct` normal que implementa el trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; falta un ajuste
  (invalid string int))     ; un valor es incorrecto

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` devuelve una descripción del error.
- `source` devuelve otro error que causó este. Sin causa, es `none`.

## 4. Combinar distintas clases de errores

Si una función llama a la vez a `parse-int` (`ParseIntError`) y a `check-workers` (`config-error`), hay
dos tipos de error, y no pueden ser ambos el `E` de un mismo `Result<T,E>`. En ese caso, haz que `E` sea
`:dyn Error` (un error de cualquier tipo que implemente `Error`). Convierte cada error con
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Dados `"4"`, `"-1"` y `"abc"`, los resultados son:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Para `:dyn`, consulta la sección 5 de [Traits](traits.md).

## 5. `panic`: errores del programa

Cuando el programa llega a un estado que nunca debería ocurrir, detenlo con `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- El tipo de `panic` es `!` (no retorna), así que se puede escribir donde se espere cualquier tipo. Por
  eso encajan las dos ramas del `if` de arriba.
- Estas operaciones también provocan un panic: `unwrap` de `none` o `err`, `get` con un índice fuera de
  rango y la división entera por cero.
- `panic` detiene el programa. Aunque ocurra dentro de una tarea, se detiene el programa entero.
- En el REPL, un `panic` no termina el REPL; espera la siguiente entrada.
- Puedes escribir `(todo)` para "aún no está escrito" y `(unreachable)` para "nunca se debería llegar aquí".
  Ambos provocan un panic.

`panic` no sustituye a `Result`. Para los fallos que pueden ocurrir, como la entrada del usuario o si
existe un archivo, usa `Result`.

## 6. `catch` / `throw`: saltar a través de funciones

`throw` salta directamente al `catch` envolvente con la misma etiqueta, por muchas llamadas a funciones
que haya en medio.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Si `v` no tiene ningún número negativo, `validate` devuelve `"all fine"`; si contiene `-7`, el control
salta desde dentro de `check-all` hasta el `catch`, que devuelve `"negative: -7"`.

- Escribe la etiqueta como un símbolo simple, como `'bad-input`.
- **Cada etiqueta transporta valores de un único tipo.** En el ejemplo de arriba `'bad-input` transporta un
  `string`, así que lanzar un `int` con la misma etiqueta es un error de tipos. El tipo del cuerpo del
  `catch` también debe coincidir con el tipo de la etiqueta.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Un `throw` sin ningún `catch` con la misma etiqueta al que llegar es un error.

Si solo quieres volver antes dentro de una función, usa `return-from` en lugar de `catch` / `throw`.
`return-from` no puede cruzar funciones, pero a cambio se ve adónde vuelve leyendo el código fuente.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: limpiar siempre

`(unwind-protect cuerpo limpieza)` ejecuta la limpieza se salga como se salga del cuerpo: cuando termina
normalmente, cuando se sale con `throw` y cuando provoca un panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Úsalo para cosas como "cerrar siempre un archivo que abriste" o "liberar siempre un cerrojo que tomaste".
`with-open-file` y `with-lock` de la biblioteca estándar usan `unwind-protect` internamente.

## 8. Sobre el sistema de condiciones de Common Lisp

typelisp no adopta el sistema de condiciones de Common Lisp (`handler-case`, `restart-case`, etc.). No
muestra en los tipos qué fallos puede causar una función, lo que encaja mal con el tipado estático. Los
fallos que pueden ocurrir se escriben en los tipos con `Result`, y las transferencias de control se hacen
con `catch` / `throw`.

## 9. Qué leer después

- [Concurrencia](concurrency.md): tareas y canales
- [Option, Result y tipos de error](../reference/functions/option-result.md): la lista de funciones
- [Mensajes de error](../reference/errors.md): qué significan los errores habituales y cómo corregirlos

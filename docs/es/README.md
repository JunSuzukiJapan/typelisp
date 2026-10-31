<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# Documentación de typelisp (español)

typelisp es un Lisp con tipado estático. Para instalarlo y compilarlo, consulta el
[README.md](../../README.md) (en inglés) en la raíz del repositorio.

## Características

### Tareas y canales

Inicia tareas (hilos ligeros) y pasa valores entre ellas a través de canales. Consulta
[Concurrencia](tutorial/concurrency.md) para más detalles.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Genéricos

Los parámetros de tipo se escriben entre corchetes angulares tras el nombre de un tipo o de una
función. Las restricciones de trait van en una cláusula `:where`. Consulta
[Fundamentos de tipos](tutorial/types.md) y [Traits](tutorial/traits.md) para más detalles.

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option y Result en lugar de nil

No existe `nil`. Una función que puede no tener valor devuelve `Option<T>`, y una que puede fallar
devuelve `Result<T,E>`; el contenido se extrae con `match`. Usar un `Option<string>` como `string`,
u omitir el caso `none` en un `match`, es un error de tipos antes de que el programa se ejecute.
Consulta [Fundamentos de tipos](tutorial/types.md) y [Manejo de errores](tutorial/errors.md) para
más detalles.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Encadenar con `->`

La macro `->` pasa un valor, por turnos, como primer argumento de cada forma siguiente.
`(-> x (f a) (g b))` equivale a `(g (f x a) b)`. Así, los pasos que devuelven `Option` o `Result`
se escriben de arriba abajo. Consulta [Option, Result y tipos de error](reference/functions/option-result.md)
para más detalles.

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### Macros con comprobación de tipos

Las macros se escriben con `defmacro`, cuasicita y `gensym`, como en Common Lisp. El código
expandido se comprueba igual que el código escrito a mano y, si no encaja con los tipos, el error se
indica en el lugar donde se llamó a la macro. Consulta [Macros](tutorial/macros.md) para más
detalles.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutorial

Si es tu primer contacto con typelisp, léelos en este orden.

- [Primeros pasos](tutorial/intro.md): el REPL, funciones, variables, condicionales, bucles, listas y `Vector`
- [Fundamentos de tipos](tutorial/types.md): tipos estáticos, `Option`, `Result`, estructuras, enumeraciones, genéricos
- [Traits](tutorial/traits.md): `deftrait` / `impl`, restricciones de trait, `:dyn`
- [Macros](tutorial/macros.md): `defmacro`, cuasicita, `gensym`, `macrolet`
- [Manejo de errores](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concurrencia](tutorial/concurrency.md): tareas, canales, `select`, `Mutex`, `thread`

## Guías

- [Módulos y organización de archivos](guide/modules.md): `use`, `pub`, cómo corresponden los archivos a los módulos
- [Compilación](guide/compile.md): el JIT, crear ejecutables con compilación AOT, volcados
- [E/S de archivos, streams y red](guide/io.md): archivos, nombres de ruta, TCP / TLS / UDP, resolución de nombres
- [FFI de C](guide/ffi.md): llamar a funciones de C con `defffi` (incluidos callbacks y structs de C con `def-c-struct`)
- [Integración con editores](guide/editors.md): `typl-lsp` y la configuración de VS Code / Emacs
- [Para programadores de Common Lisp](guide/from-common-lisp.md): en qué se diferencia typelisp de CL y cómo reescribir código de CL

## Referencia

- [Referencia de sintaxis](reference/syntax.md): léxico, escritura de tipos, definiciones, formas de control, compilación, concurrencia
- [Funciones incorporadas](reference/functions/README.md): funciones incorporadas, métodos y la biblioteca estándar
- [Tipos](reference/types.md): los tipos y los traits que implementa cada uno
- [Mensajes de error](reference/errors.md): qué significan los errores habituales y cómo corregirlos

## Integración con editores

Los pasos de configuración están en la [guía de integración con editores](guide/editors.md). Los atajos
de teclado y los ajustes de cada editor se describen en estos documentos:

- [Emacs (typelisp-mode)](../../editor/emacs/README_es.md)
- [VS Code](../../editor/vscode/README_es.md)

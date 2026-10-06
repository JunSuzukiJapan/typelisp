<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Módulos y organización de archivos

Esta guía explica cómo montar un programa formado por varios archivos. Las reglas detalladas están en las
secciones 3.10 a 3.13 de la [Referencia de sintaxis](../reference/syntax.md#310-module--use--espacios-de-nombres).

## 1. Un archivo es un módulo

En typelisp, **un archivo es un módulo por sí mismo**. La ruta del archivo relativa a la raíz de fuentes
es la ruta del módulo.

| Archivo | Módulo |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

No hace falta escribir una declaración de módulo al principio del archivo.

## 2. Preparar un proyecto

Coloca un archivo llamado `typelisp.toml` en la raíz del proyecto. Puede estar vacío.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Para tener las fuentes bajo `src/`, escribe esta línea en `typelisp.toml`:

```toml
src = "src"
```

`typl` busca `typelisp.toml` empezando por el directorio del archivo que ejecuta y subiendo, y usa el lugar
donde lo encuentra como raíz de fuentes. Si no encuentra ninguno, la raíz es el directorio del archivo que
se ejecuta (en el REPL, el directorio actual).

## 3. Hacer públicas las definiciones y usarlas

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; un campo sin pub no se puede leer desde fuera

(defun square ((n i32)) i32 (* n n))   ; una función sin pub tampoco se puede llamar desde fuera

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` se carga en el punto donde se escribe `(use geometry)`. No hace falta cargarlo antes.

### Qué se hace público

- Las funciones, estructuras, enumeraciones, variables globales, macros y métodos son visibles desde otros
  módulos solo cuando llevan `pub`. Pon `pub` justo antes de la definición, como en `(pub defun ...)`.
- En las estructuras, **hacer público el tipo y hacer públicos los campos son cosas distintas**.
  `(pub defstruct point ...)` hace visible el tipo, y solo los campos escritos como `(pub x i32)` se pueden
  leer y escribir desde fuera.
- Usar desde fuera un nombre que no es público da un error de "no se puede resolver" como
  `unresolved path: geometry::square`. Es el mismo mensaje que para un nombre mal escrito, así que si la
  ortografía es correcta y el nombre sigue sin resolverse, sospecha que falta un `pub`.

La lista de definiciones que pueden llevar `pub` está en
[Referencia de sintaxis 3.13](../reference/syntax.md#313-pub--visibilidad).

## 4. Cómo escribir `use`

```lisp
(use geometry)              ; traer un módulo; para usarlo se escribe geometry::dist2
(use geometry::dist2)       ; traer una función; se usa por el nombre simple dist2
(use geometry::point)       ; traer un tipo; point::new, point::origin y point en las anotaciones de tipo
(use a::f b::g)             ; se pueden escribir varios juntos
```

- **`use` solo afecta a las formas que van detrás.** Ponlo al principio del archivo. Escribir
  `geometry::dist2` por encima del `use` da `unresolved path`.
- Escribir la ruta completa `geometry::dist2` sin hacer `use` del módulo tampoco se resuelve. Solo `use`
  hace que se cargue un archivo.
- Hacer `use` de un nombre cuya forma simple ya está en uso da un aviso. Cuando quieras traerlo de todos
  modos, usa `shadowing-import`.
- Un módulo dentro de un directorio se escribe `(use geo::shapes)`, y a partir de ahí se le llama por su
  última parte (`shapes::...`).

### Llamar a métodos de traits

Los métodos implementados en un `impl` **pertenecen al tipo**, no a las funciones del módulo, así que se
llaman sin el nombre del módulo.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, no core::area
```

Los métodos dentro de un `impl` son siempre públicos, aunque no lleven `pub`.

Un trait en sí no se puede hacer público a otros módulos. Mantén la definición de un trait, sus `impl` y el
código que lo usa mediante `:dyn` en un solo módulo.

## 5. Dividir espacios de nombres dentro de un archivo

Para dividir aún más un espacio de nombres dentro de un archivo, usa `module`. Queda anidado dentro del
módulo propio del archivo.

```lisp
;; dentro de main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Para meter todo el resto del archivo en un espacio de nombres, puedes escribir `(in-module util)` en lugar
de envolverlo entre paréntesis.

## 6. Restricciones de dependencias

- **No se permiten ciclos.** Si `a.typl` hace `(use b)` y `b.typl` hace `(use a)`, el resultado es el error
  `circular module dependency: a -> b -> a`. Mueve las definiciones que necesitan ambos a un tercer módulo.
- **Ni los tipos ni las funciones se pueden referenciar antes de definirse**, incluso dentro del mismo
  archivo. Para funciones mutuamente recursivas, declara una de ellas primero con `defsignature`
  ([Referencia de sintaxis 3.2](../reference/syntax.md#32-defsignature--declaraciones-anticipadas)).

## 7. Orden de ejecución

Ejecutar `typl main.typl` procede en este orden:

1. Se leen y se comprueban los tipos de `main.typl` y de todos los archivos que usa con `use`. **Si hay un
   error de tipos en cualquier parte, no se ejecuta nada.**
2. Las expresiones de nivel superior de los módulos usados se ejecutan antes que las de los módulos que los
   usan.
3. Las expresiones de nivel superior de `main.typl` se ejecutan de arriba abajo.

Si reúnes el punto de entrada del programa en una función `main` y llamas a `(main)` al final del archivo,
el mismo archivo también sirve para la
[compilación AOT](compile.md#3-construir-un-ejecutable-con-compilación-aot).

## 8. En qué se diferencia de `load`

`(load "ruta")`, como el `load` de Common Lisp, lee el contenido de un archivo **en el espacio de nombres
actual tal cual**. No lo envuelve en un módulo, y `pub` no interviene. Úsalo para cosas como leer un
archivo de configuración o recargar un archivo local en el REPL. Para dividir un programa en partes, usa
`use`.

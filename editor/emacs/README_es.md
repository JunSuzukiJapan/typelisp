<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

Un modo mayor de Emacs para editar código fuente de typelisp (`.typl`).
La versión para VS Code está en [../vscode/](../vscode/README_es.md). Ambas comparten las mismas
tablas de palabras clave y las mismas reglas de sangría, y `cargo test --test editor_keyword_sync_test`
lo comprueba de forma mecánica (véase el final de este documento).

## Funciones

- Resaltado de sintaxis
  - Formas especiales y construcciones de control (`defun` `let` `if` `match` `loop` `lambda`
    `setf` `as` `apply`, `print`/`println`/`format`, la familia `pprint`, etc.)
  - Nombres definidos (el `NAME` de `(defun NAME ...)` como nombre de función, el de
    `(defstruct NAME ...)` como nombre de tipo y el de `(defvar (NAME ...))` como nombre de
    variable; lo mismo con `pub`, como en `(pub defun NAME ...)`)
  - Palabras clave de espacios de nombres y declaraciones (`pub` `module` `use` `load` `impl`
    `where`) y marcadores de lista lambda (`&rest` `&optional` `&key`)
  - Funciones integradas (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car`, etc.)
  - Tipos primitivos (incluidos `bignum` / `ratio`), tipos integrados, tipos de error integrados
    (`ParseIntError`, etc.), tipos de usuario `Capitalized` y el tipo de objeto trait `:dyn Trait`
  - **Usos de tipos definidos por el usuario** (los nombres de `defstruct`/`defenum`/`deftrait`
    suelen ir en minúsculas (`rect` `todo-item` `board`), así que la regla `Capitalized` no los
    captura). Con conexión a `typl-lsp`, se colorean a partir de los semantic tokens del servidor
    (también funciona con `eglot`; véase más abajo). Sin conexión, el modo recurre a reunir los
    nombres de tipo definidos en el búfer
  - Literales (`true` `false`, literales numéricos (decimal / `0xff` / `1.5` / `1/3`), literales
    de carácter como `#\Space`, cadenas, palabras clave como `:name`)
  - Directivas de control de `format` dentro de cadenas (`~a` `~5,'0d` `~{...~}`, etc.)
  - Variables globales con orejeras al estilo de CL (`*print-pretty*`, etc.)
- Comentarios
  - Comentarios de línea `;`
  - Comentarios de bloque **anidables** `#| ... |#`
- Navegación por expresiones S y sangría al estilo Lisp
- Índice de definiciones mediante `imenu` (funciones / métodos / macros / tipos / traits / `impl` /
  variables / módulos)
- Órdenes que ejecutan la CLI `typl` (más abajo)

## Atajos de teclado

| Tecla | Orden | Qué hace |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Guarda y ejecuta `typl FILE` (a través de `compile`, así que se puede saltar a las líneas con error) |
| `C-c C-z` | `typelisp-repl` | Inicia el REPL de `typl` en un búfer comint |

La ubicación de `typl` se indica con `typelisp-program` (por defecto `"typl"`).
Los diagnósticos tienen la forma `error: FILE:LINE:COL: ...`, que `compilation-mode` sabe
interpretar, así que `next-error` / `C-x \`` salta directamente al lugar.

## Instalación

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Los archivos `.typl` se abren automáticamente en `typelisp-mode` (el modo está registrado en
`auto-mode-alist`).

Con `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Servidor de lenguaje (`typl-lsp`)

Una vez compilado `typl-lsp`, se puede usar desde `eglot` (incluido en Emacs 29+) o `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Con `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Con `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Admite diagnósticos (errores de sintaxis y de tipos y avisos de redefinición, enviados mediante
`textDocument/publishDiagnostics`), hover, ir a la definición (goto-definition), completado (`:`
está registrado como carácter de activación) y semantic tokens. Las referencias entre archivos a
través de `use` se resuelven (el servidor busca hacia arriba el `typelisp.toml` de la raíz del
proyecto; para más detalles, véase
[Referencia de sintaxis 3.11](../../docs/es/reference/syntax.md#311-archivos-y-módulos-proyectos-de-varios-archivos)).
Las ediciones sin guardar de los búferes abiertos se reflejan de inmediato en los diagnósticos
tanto de los archivos de los que dependen como de los que dependen de ellos.

### Resaltado de nombres de tipo (semantic tokens)

Mediante `textDocument/semanticTokens`, el servidor informa de **las posiciones que el
comprobador resolvió realmente como nombres de tipo**. Como no es una comparación de texto:

- Los tipos que llegan de otros archivos a través de `use` también se colorean (un alcance al que
  la resolución dentro del búfer no puede llegar por principio)
- Las llamadas a una **función** con el mismo nombre que un tipo no se colorean (el comprobador las
  resolvió como funciones, así que ni siquiera se registra un token allí)

En el lado del cliente:

- **`eglot` (Emacs 31 y posteriores)**: eglot dibuja los tokens por sí mismo
  (`eglot-semantic-tokens-mode`). `typelisp-mode` no interviene
- **`eglot` (Emacs 30 y anteriores)**: esta versión de eglot no maneja semanticTokens. Por eso
  **`typelisp-mode` envía la petición por su cuenta y dibuja el resultado con overlays**
  (`typelisp-semantic-tokens-mode`, que se activa automáticamente al conectar eglot)
- **`lsp-mode`**: soporte nativo (ponga `lsp-semantic-tokens-enable` a `t`). En ese caso
  `typelisp-mode` no interviene

`scripts/emacs-semantic-smoke.el` se conecta de verdad mediante eglot y comprueba la parte que se
encarga de dibujar en el Emacs que se esté usando. Con cualquier cliente, el método alternativo
dentro del búfer se retira mientras el servidor responde (para que dos conjuntos de reglas no
pinten el mismo búfer).

| Opción | Valor por defecto | Qué hace |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Con el eglot de Emacs 30 y anteriores, si se colorea a partir de los semantic tokens del servidor |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Segundos de inactividad tras una edición antes de volver a pedir (debe ser mayor que `eglot-send-changes-idle-time`) |

## Notas

- typelisp pasa los símbolos a minúsculas al leerlos, pero el resaltado distingue mayúsculas para
  poder separar los nombres de tipo que empiezan por mayúscula.
- La sangría la decide la función dedicada `typelisp-indent-function`, que consulta
  `typelisp-indent-specs` (una lista asociativa). El modo guarda sus propias entradas incluso para
  las formas cuyo nombre comparte con Emacs Lisp (`defun` `let` `if` ...) porque las propiedades
  de símbolo son **globales**, y una configuración para typelisp ahí cambiaría la sangría de otros
  búferes Lisp de la misma sesión. Además, las formas de typelisp difieren en forma aunque
  compartan nombre con Emacs Lisp —`(defun NAME (PARAMS) RETTYPE ...)` tiene tres elementos de
  cabecera, e `if` tiene fijos tres elementos con un `else` obligatorio—, así que los valores
  tampoco se pueden compartir.
  Se ha comprobado que ningún archivo `.typl` de `examples/` cambia un solo byte con
  `indent-region`, y que quitar toda la sangría y volver a sangrar restaura el original (la
  versión para VS Code cumple el mismo criterio con los mismos archivos).

## Detección de desfases en las definiciones del editor

Las tablas de palabras clave se mantienen por duplicado, aquí y en la versión para VS Code. Para
evitar que las definiciones del editor se queden atrás mientras avanza la implementación, hay una
prueba en el lado de Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Carga de verdad el prelude, recorre el registro e informa de **los nombres que alguno de los
editores no conoce**. Las formas especiales no tienen representación en tiempo de ejecución, así
que se leen de entre `// SPECIAL-FORM DISPATCH BEGIN` / `END` en
`crates/typelisp-front/src/check/checker.rs` (no borre estos comentarios). Si falla, añada los
nombres indicados a **ambas** definiciones de editor.

La misma prueba compara también la leyenda de semantic tokens (`SEMANTIC_TOKEN_TYPES` en
`src/bin/lsp.rs` y las tablas de ambos editores deben coincidir en nombres y orden). Un desajuste
no provoca ningún error en tiempo de ejecución; solo intercambia los colores de todos los tokens,
por eso se fija de forma mecánica.

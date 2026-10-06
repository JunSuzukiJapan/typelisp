<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

Una extensión de VS Code para editar código fuente de typelisp (`.typl`).
La versión para Emacs está en [../emacs/](../emacs/README_es.md). Ambas comparten las mismas tablas
de palabras clave y las mismas reglas de sangría, y `cargo test --test editor_keyword_sync_test` lo
comprueba de forma mecánica (véase más abajo).

## Funciones

- **Resaltado de sintaxis** (una gramática TextMate; no necesita servidor de lenguaje)
  - Formas especiales y construcciones de control (`defun` `let` `if` `match` `loop` `lambda`
    `setf` `as` `apply`, `print`/`println`/`format`, la familia `pprint`)
  - Nombres definidos (`(defun NAME ...)` como función, `(defstruct NAME ...)` como tipo,
    `(defvar (NAME ...))` como variable; lo mismo con `pub`, como en `(pub defun NAME ...)`) y los
    dos nombres de `(impl Trait Type)`
  - Palabras clave de espacios de nombres y declaraciones (`pub` `module` `use` `load` `impl`
    `where`) y marcadores de lista lambda (`&rest` `&optional` `&key`)
  - Funciones integradas, tipos primitivos (incluidos `bignum` / `ratio`), tipos de error
    integrados, tipos de usuario `Capitalized` y el tipo de objeto trait `:dyn Trait` (también
    dentro de argumentos genéricos)
  - Literales numéricos (decimal / `0xff` / `1.5` / `3.0e10` / `1/3`), literales de carácter como
    `#\Space`, palabras clave como `:name` y variables globales con orejeras como `*print-pretty*`
  - **Directivas de control de `format` dentro de cadenas** (`~a` `~5,'0d` `~{...~}` `~^`, etc.)
  - Comentarios de línea `;` y comentarios de bloque **anidables** `#| ... |#`
- **Usos de tipos definidos por el usuario** (semantic tokens)
  - Los nombres de `defstruct` / `defenum` / `deftrait` suelen ir en minúsculas (`rect`
    `todo-item` `board`), así que la regla `Capitalized` no los captura, y una gramática TextMate
    trabaja línea a línea y no puede ver el archivo entero. Los semantic tokens sí pueden, lo que
    resuelve la situación de un lenguaje con tipado estático en el que solo sus anotaciones de tipo
    quedaban sin color
  - Con conexión a `typl-lsp`, la extensión recibe **las posiciones que el comprobador resolvió
    realmente como nombres de tipo**. Así, los tipos que llegan de otros archivos a través de `use`
    se colorean, y las llamadas a una **función** con el mismo nombre que un tipo no (el
    comprobador las resolvió como funciones, así que ni siquiera se registra un token allí)
  - Cuando el servidor no está conectado o no está compilado, la extensión recurre a un análisis
    de texto que resuelve dentro del archivo. Es una aproximación: no encuentra tipos de otros
    archivos ni distingue una función con el mismo nombre que un tipo
- **Sangría Lisp** (VS Code no tiene sangría Lisp de serie, así que la implementa la extensión)
  - Dar formato al documento, dar formato a la selección y formato al escribir (Enter y `)`, con
    `editor.formatOnType` activado)
- **Outline / breadcrumbs / `Ctrl+Shift+O`** (funciones, métodos, macros, tipos, traits, `impl`,
  variables, módulos)
- **Integración con `typl-lsp`** (diagnósticos, hover, ir a la definición, completado, semantic
  tokens)
- **Órdenes de la CLI `typl`** (ejecutar, REPL)

Todo salvo el servidor de lenguaje funciona solo con la extensión, así que incluso en una copia en
la que no se haya compilado `typl-lsp` están disponibles el resaltado, la sangría, el Outline y el
resaltado de tipos (limitado al archivo).

## Instalación

La extensión no está en el Marketplace, así que hay que compilarla e instalarla localmente.

```sh
cd editor/vscode
npm install
npm run compile
```

Después, una de estas dos opciones:

- **Probarla en un host de desarrollo**: abra `editor/vscode` en VS Code y pulse `F5`
- **Instalarla de forma permanente**: cree un `.vsix` con `npx @vscode/vsce package` y luego use
  "..." → "Install from VSIX..." en la vista de extensiones

Los archivos `.typl` se abren automáticamente en el modo typelisp.

## Atajos de teclado

| Tecla | Orden | Qué hace |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Guarda y ejecuta `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Inicia el REPL de `typl` |

La paleta de comandos también tiene `typelisp: Restart Language Server`.

## Configuración

| Opción | Valor por defecto | Qué hace |
|---|---|---|
| `typelisp.program` | `typl` | Ruta de la CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Si se conecta a `typl-lsp` |
| `typelisp.languageServer.path` | (vacío) | Ruta de `typl-lsp`. Si está vacía, se busca en este orden: `target/release/typl-lsp` del espacio de trabajo, `target/debug/typl-lsp` y `PATH` |
| `typelisp.trace.server` | `off` | Registra el tráfico JSON-RPC de LSP |

El servidor de lenguaje se compila con:

```sh
cargo build --release --bin typl-lsp
```

Las referencias entre archivos a través de `use` se resuelven buscando hacia arriba el
`typelisp.toml` de la raíz del proyecto (para más detalles, véase
[Referencia de sintaxis 3.11](../../docs/es/reference/syntax.md#311-archivos-y-módulos-proyectos-de-varios-archivos)).

## El problem matcher de tareas

La extensión ofrece un problem matcher llamado `typelisp`. `typl` imprime los diagnósticos con la
forma `error: FILE:LINE:COL: message`, así que pueden ir directamente al panel Problemas:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## Desarrollo

```sh
npm run compile   # tsc
npm run watch     # compilación al detectar cambios
npm test          # node --test (gramática, sangría, símbolos, referencias de tipo, manifiesto)
```

Las pruebas cubren solo las partes que no necesitan el módulo `vscode`. Para ello, `src/indent.ts`
y `src/symbols.ts` están escritos como funciones puras, y solo `src/extension.ts` toca la API del
editor.

- `src/test/grammar.test.ts` — tokeniza de verdad con la gramática, usando el mismo motor que VS
  Code (`vscode-textmate` + `vscode-oniguruma`), y comprueba el resultado.
  Oniguruma difiere de las expresiones regulares de Emacs en detalles (por ejemplo, no trata como
  literal un `]` al principio de una clase de caracteres), y esas diferencias solo aparecen al
  ejecutar el motor real.
- `src/test/indent.test.ts` — para cada archivo `.typl` de `examples/`, exige que **quitar toda la
  sangría y restaurarla coincida byte a byte con el contenido confirmado**.
  El modo de Emacs cumple el mismo criterio con los mismos archivos, y eso es lo que convierte
  "los dos editores coinciden" en una afirmación comprobada.
  Además, `src/test/fixtures/emacs-indent-reference.txt` es una salida de referencia obtenida
  ejecutando de verdad `indent-region` en un búfer `typelisp-mode` de Emacs. El valor esperado no
  es una repetición de la implementación en TS sino **lo que produce realmente el otro editor**,
  así que la fidelidad de la adaptación se comprueba directamente (incluye `let*` `do` `doiter`
  `labels` `impl` `pprint-logical-block`, prefijos de quote y más).
- `src/test/symbols.test.ts` — el contenido del Outline y la detección de referencias de tipo del
  método alternativo. El número de definiciones debe coincidir exactamente con un recuento
  independiente de las formas de definición al principio de las líneas. Las reglas de frontera de
  las referencias de tipo están alineadas a propósito con el método alternativo de la versión para
  Emacs (VS Code usa un lookbehind; Emacs expresa el mismo conjunto consumiendo un carácter
  anterior).
- Los tokens del servidor guiados por la resolución (`crates/typelisp-front/src/check/semantic.rs`)
  los comprueban `cargo test --test lsp_semantic_test` y `scripts/lsp-semantic-smoke.py` (que
  maneja un proceso real por stdio). El cliente de Emacs lo comprueba
  `scripts/emacs-semantic-smoke.el` con una conexión real de eglot.
- `src/test/manifest.test.ts` — `package.json` es la única parte que el compilador no comprueba,
  así que esta prueba verifica que las órdenes declaradas y las llamadas a `registerCommand` forman
  el mismo conjunto, a qué se refieren los atajos de teclado, que las opciones que lee el código
  están declaradas y que el problem matcher sabe interpretar lo que `typl` imprime realmente.

### Detección de desfases en las definiciones del editor

Las tablas de palabras clave se mantienen por duplicado, en la versión para Emacs y en la de VS
Code. Para evitar que las definiciones del editor se queden atrás mientras avanza la
implementación, hay una prueba en el lado de Rust:

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

## Notas

- typelisp pasa los símbolos a minúsculas al leerlos, pero el resaltado distingue mayúsculas para
  poder separar los nombres de tipo que empiezan por mayúscula.
- La sangría la decide `INDENT_SPECS` en `src/indent.ts`. Es una adaptación de
  `typelisp-indent-specs` de la versión para Emacs, con los mismos valores y reglas. Se conservan
  tal cual los puntos en que una forma difiere de la forma homónima de Emacs Lisp: la cabecera de
  `(defun NAME (PARAMS) RETTYPE ...)` tiene tres elementos, `if` tiene fijos tres elementos con un
  `else` obligatorio, etc.
- El contenido de `#| ... |#` se vuelve a sangrar al dar formato. Coincide con el comportamiento de
  `indent-region` de Emacs.

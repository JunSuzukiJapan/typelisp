<!-- translated-from: docs/ja/guide/editors.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Integración con editores (typl-lsp)

`typl-lsp` es el servidor de lenguaje de typelisp. Conectado a un editor compatible con LSP (el Language
Server Protocol), ofrece estas funciones para el archivo que estás editando:

- Diagnósticos: errores de lectura, errores de tipos y avisos de redefinición
- Información al pasar el ratón: el tipo de una expresión entre paréntesis y la docstring de la definición
  a la que llama (no se muestra para nombres de variable simples)
- Ir a la definición
- Autocompletado (los candidatos aparecen al teclear `:`)
- Coloreado de nombres de tipo (tokens semánticos), incluidos los tipos traídos con `use` de otros archivos

Las referencias entre archivos mediante `use` se resuelven. Las ediciones sin guardar de otro archivo
abierto se reflejan enseguida en los diagnósticos de los archivos que lo usan con `use`.

## 1. Compilación

```sh
cargo build --release --bin typl-lsp
```

Esto produce `target/release/typl-lsp`. Si instalaste con `cargo install` como se describe en el
[README.md](../../../README.md), está en `~/.cargo/bin/typl-lsp` junto a `typl`.

## 2. VS Code

La extensión está en `editor/vscode` del repositorio. No está publicada en el Marketplace, así que
compílala e instálala tú.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produce un .vsix
```

Elige "Install from VSIX..." en el menú "..." de la vista de extensiones y selecciona el `.vsix` que
compilaste.

La extensión busca `typl-lsp` en `target/release/typl-lsp` del espacio de trabajo, después en
`target/debug/typl-lsp` y después en el `PATH`. Si lo pones en otro sitio, escribe su ruta en el ajuste
`typelisp.languageServer.path`.

| Ajuste | Valor por defecto | Significado |
|---|---|---|
| `typelisp.program` | `typl` | Ruta de `typl` |
| `typelisp.languageServer.enable` | `true` | Si se conecta a `typl-lsp` |
| `typelisp.languageServer.path` | (vacío) | Ruta de `typl-lsp` |

`Ctrl+Alt+R` guarda el archivo que estás editando y lo ejecuta con `typl`, y `Ctrl+Alt+Z` arranca el REPL.
Para más, consulta el [README de la extensión de VS Code](../../../editor/vscode/README_es.md).

## 3. Emacs

`typelisp-mode` está en `editor/emacs` del repositorio.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Ajustes para conectar con `typl-lsp` mediante `eglot` (incluido en Emacs 29 y posteriores):

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

El eglot de Emacs 31 y posteriores colorea por sí mismo los nombres de tipo (tokens semánticos). El eglot de
Emacs 30 y anteriores no los admite, así que `typelisp-mode` colorea los nombres de tipo en su lugar. Con
`lsp-mode`, pon `lsp-semantic-tokens-enable` a `t`.

`C-c C-c` ejecuta el archivo que estás editando y `C-c C-z` arranca el REPL. Para más, consulta el
[README de typelisp-mode](../../../editor/emacs/README_es.md).

## 4. Otros editores

`typl-lsp` habla LSP por la entrada y la salida estándar y no recibe argumentos de línea de órdenes.
Configura el cliente LSP de tu editor para que arranque `typl-lsp` para los archivos `.typl`.

## 5. Cómo se reconocen los proyectos

`typl-lsp` busca `typelisp.toml` empezando por el directorio del archivo abierto y subiendo, y resuelve
`use` tomando ese lugar como raíz de fuentes. Son las mismas reglas que cuando `typl` ejecuta un archivo
([Módulos y organización de archivos](modules.md#2-preparar-un-proyecto)). Para un proyecto de varios
archivos, coloca `typelisp.toml` en su raíz.

## 6. El servidor de lenguaje no ejecuta tu programa

`typl-lsp` produce los diagnósticos solo leyendo y comprobando tipos. Nunca ejecuta el programa que estás
editando. Los diagnósticos se calculan con cada pulsación, así que no puede permitirse ejecutar ahí código
con efectos secundarios o código que nunca termina. La única excepción es el registro de los `defmacro`,
necesario para comprobar las llamadas a macros que vienen después.

Por eso, los errores que solo ocurren cuando `typl` ejecuta el programa (`panic`, un archivo que falta,
etc.) no aparecen en los diagnósticos del servidor de lenguaje.

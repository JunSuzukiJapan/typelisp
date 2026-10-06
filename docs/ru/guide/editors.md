<!-- translated-from: docs/ja/guide/editors.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Интеграция с редакторами (typl-lsp)

`typl-lsp` — языковой сервер typelisp. Подключённый к редактору, поддерживающему LSP (Language Server Protocol), он
предоставляет для редактируемого файла такие возможности:

- Диагностика: ошибки чтения, ошибки типов и предупреждения о переопределении
- Наведение: тип выражения в скобках и строка документации определения, которое оно вызывает (для голых имён
  переменных не показывается)
- Переход к определению
- Автодополнение (кандидаты появляются при вводе `:`)
- Раскраска имён типов (семантические токены), включая типы, подключённые через `use` из других файлов

Ссылки между файлами через `use` разрешаются. Несохранённые правки другого открытого файла сразу отражаются в
диагностике файлов, которые подключают его через `use`.

## 1. Сборка

```sh
cargo build --release --bin typl-lsp
```

В результате получается `target/release/typl-lsp`. Если вы установили через `cargo install`, как описано в
[README.md](../../../README.md), он лежит в `~/.cargo/bin/typl-lsp` рядом с `typl`.

## 2. VS Code

Расширение находится в `editor/vscode` в репозитории. Оно не опубликовано в Marketplace, поэтому соберите и
установите его сами.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # создаёт .vsix
```

В меню «...» панели Extensions выберите «Install from VSIX...» и укажите собранный `.vsix`.

Расширение ищет `typl-lsp` в `target/release/typl-lsp` рабочей области, затем в `target/debug/typl-lsp`, затем в
`PATH`. Если он лежит в другом месте, укажите путь в настройке `typelisp.languageServer.path`.

| Настройка | По умолчанию | Смысл |
|---|---|---|
| `typelisp.program` | `typl` | Путь к `typl` |
| `typelisp.languageServer.enable` | `true` | Подключаться ли к `typl-lsp` |
| `typelisp.languageServer.path` | (пусто) | Путь к `typl-lsp` |

`Ctrl+Alt+R` сохраняет редактируемый файл и запускает его через `typl`, а `Ctrl+Alt+Z` запускает REPL. Подробнее см.
[README расширения VS Code](../../../editor/vscode/README_ru.md).

## 3. Emacs

`typelisp-mode` находится в `editor/emacs` в репозитории.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Настройки для подключения к `typl-lsp` через `eglot` (входит в Emacs 29 и новее):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

С `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot из Emacs 31 и новее сам раскрашивает имена типов (семантические токены). eglot из Emacs 30 и старше их не
поддерживает, поэтому имена типов вместо него раскрашивает `typelisp-mode`. С `lsp-mode` установите
`lsp-semantic-tokens-enable` в `t`.

`C-c C-c` запускает редактируемый файл, а `C-c C-z` запускает REPL. Подробнее см.
[README typelisp-mode](../../../editor/emacs/README_ru.md).

## 4. Другие редакторы

`typl-lsp` говорит по LSP через стандартный ввод и вывод и не принимает аргументов командной строки. Настройте
LSP-клиент своего редактора так, чтобы для файлов `.typl` запускался `typl-lsp`.

## 5. Как распознаются проекты

`typl-lsp` ищет `typelisp.toml`, начиная с каталога открытого файла и поднимаясь вверх, и разрешает `use`, считая
найденное место корнем исходников. Это те же правила, что и при запуске файла через `typl`
([Модули и разбиение на файлы](modules.md#2-настройка-проекта)). Для проекта из нескольких файлов положите
`typelisp.toml` в его корень.

## 6. Языковой сервер не запускает вашу программу

`typl-lsp` формирует диагностику только чтением и проверкой типов. Он никогда не запускает редактируемую программу.
Диагностика выполняется при каждом нажатии клавиши, поэтому позволить себе выполнять там код с побочными эффектами
или код, который никогда не завершится, нельзя. Единственное исключение — регистрация `defmacro`, нужная для проверки
следующих за ними вызовов макросов.

Поэтому ошибки, возникающие только при запуске программы через `typl` (`panic`, отсутствующий файл и т. п.), в
диагностике языкового сервера не появляются.

<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Інтеграція з редакторами (typl-lsp)

`typl-lsp` — це мовний сервер typelisp. Підключений до редактора з підтримкою LSP (Language Server
Protocol), він надає для файлу, що редагується, такі можливості:

- Діагностика: помилки читання, помилки типів і попередження про перевизначення
- Наведення: тип виразу в дужках і рядок документації визначення, яке він викликає (для голих імен
  змінних не показується)
- Перехід до визначення
- Автодоповнення (кандидати з'являються, коли ви вводите `:`)
- Розфарбовування імен типів (семантичні токени), зокрема типів, підключених через `use` з інших файлів

Посилання між файлами через `use` розв'язуються. Незбережені правки іншого відкритого файлу одразу
відображаються в діагностиці файлів, які підключають його через `use`.

## 1. Збирання

```sh
cargo build --release --bin typl-lsp
```

У результаті виходить `target/release/typl-lsp`. Якщо ви встановили через `cargo install`, як описано в
[README.md](../../../README.md), він лежить у `~/.cargo/bin/typl-lsp` поруч із `typl`.

## 2. VS Code

Розширення міститься в `editor/vscode` у репозиторії. Воно не опубліковане в Marketplace, тому зберіть
і встановіть його самостійно.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # створює .vsix
```

У меню «...» подання Extensions виберіть «Install from VSIX...» і вкажіть зібраний `.vsix`.

Розширення шукає `typl-lsp` у `target/release/typl-lsp` робочого простору, потім у
`target/debug/typl-lsp`, потім у `PATH`. Якщо ви поклали його деінде, запишіть шлях до нього в
налаштування `typelisp.languageServer.path`.

| Налаштування | Типове значення | Значення |
|---|---|---|
| `typelisp.program` | `typl` | Шлях до `typl` |
| `typelisp.languageServer.enable` | `true` | Чи підключатися до `typl-lsp` |
| `typelisp.languageServer.path` | (порожньо) | Шлях до `typl-lsp` |

`Ctrl+Alt+R` зберігає файл, що редагується, і запускає його через `typl`, а `Ctrl+Alt+Z` запускає
REPL. Докладніше див. [README розширення VS Code](../../../editor/vscode/README_uk.md).

## 3. Emacs

`typelisp-mode` міститься в `editor/emacs` у репозиторії.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Налаштування для підключення до `typl-lsp` через `eglot` (входить до складу Emacs 29 і новіших):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

З `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot в Emacs 31 і новіших сам розфарбовує імена типів (семантичні токени). eglot в Emacs 30 і
старіших їх не підтримує, тому імена типів натомість розфарбовує `typelisp-mode`. З `lsp-mode`
установіть `lsp-semantic-tokens-enable` у `t`.

`C-c C-c` запускає файл, що редагується, а `C-c C-z` запускає REPL. Докладніше див.
[README typelisp-mode](../../../editor/emacs/README_uk.md).

## 4. Інші редактори

`typl-lsp` розмовляє за LSP через стандартний ввід і вивід і не приймає аргументів командного рядка.
Налаштуйте LSP-клієнт вашого редактора так, щоб він запускав `typl-lsp` для файлів `.typl`.

## 5. Як розпізнаються проєкти

`typl-lsp` шукає `typelisp.toml`, починаючи з каталогу відкритого файлу й рухаючись угору, і розв'язує
`use`, вважаючи це місце кореневим каталогом вихідного коду. Це ті самі правила, що й коли `typl` запускає
файл ([Модулі та розкладання по файлах](modules.md#2-налаштування-проєкту)). Для проєкту з кількох файлів
покладіть `typelisp.toml` у його корінь.

## 6. Мовний сервер не запускає вашу програму

`typl-lsp` створює діагностику лише читанням і перевіркою типів. Він ніколи не запускає програму, що
редагується. Діагностика виконується при кожному натисканні клавіші, тому дозволити собі запуск коду з
побічними ефектами або коду, що ніколи не завершується, вона не може. Єдиний виняток — реєстрація
`defmacro`, яка потрібна для перевірки викликів макросів, що йдуть після них.

Через це помилки, які трапляються лише тоді, коли `typl` запускає програму (`panic`, відсутній файл
тощо), не з'являються в діагностиці мовного сервера.

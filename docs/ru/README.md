<!-- translated-from: docs/ja/README.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Документация typelisp (русский)

typelisp — это Лисп со статической типизацией. Об установке и сборке см.
[README.md](../../README.md) (на английском) в корне репозитория.

## Учебник

Если вы впервые знакомитесь с typelisp, читайте в таком порядке.

- [Первые шаги](tutorial/intro.md): REPL, функции, переменные, ветвления, циклы, списки и `Vector`
- [Основы типов](tutorial/types.md): статические типы, `Option`, `Result`, структуры, перечисления, обобщённые типы
- [Трейты](tutorial/traits.md): `deftrait` / `impl`, ограничения трейтами, `:dyn`
- [Макросы](tutorial/macros.md): `defmacro`, квазицитирование, `gensym`, `macrolet`
- [Обработка ошибок](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Конкурентность](tutorial/concurrency.md): задачи, каналы, `select`, `Mutex`, `thread`

## Руководства

- [Модули и разбиение на файлы](guide/modules.md): `use`, `pub`, соответствие файлов и модулей
- [Компиляция](guide/compile.md): JIT, сборка исполняемых файлов AOT-компиляцией, дампы
- [Файловый ввод-вывод, потоки и сеть](guide/io.md): файлы, пути, TCP / TLS / UDP, разрешение имён
- [FFI для C](guide/ffi.md): вызов функций C через `defffi` (включая обратные вызовы и структуры C с `def-c-struct`)
- [Интеграция с редакторами](guide/editors.md): `typl-lsp` и настройка VS Code / Emacs
- [Для программистов на Common Lisp](guide/from-common-lisp.md): чем typelisp отличается от CL и как переписывать код на CL

## Справочник

- [Справочник по синтаксису](reference/syntax.md): лексика, запись типов, определения, управляющие формы, компиляция, конкурентность
- [Встроенные функции](reference/functions/README.md): встроенные функции, методы и стандартная библиотека
- [Типы](reference/types.md): типы и трейты, которые реализует каждый из них
- [Сообщения об ошибках](reference/errors.md): что означают распространённые ошибки и как их исправить

## Интеграция с редакторами

Порядок настройки описан в [руководстве по интеграции с редакторами](guide/editors.md). Привязки клавиш и
настройки каждого редактора перечислены в этих документах:

- [Emacs (typelisp-mode)](../../editor/emacs/README_ru.md)
- [VS Code](../../editor/vscode/README_ru.md)

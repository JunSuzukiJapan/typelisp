<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Документація typelisp (українська)

typelisp — це Лісп зі статичною типізацією. Про встановлення та збирання див.
[README.md](../../README.md) (англійською) у корені репозиторію.

## Підручник

Якщо ви вперше знайомитеся з typelisp, читайте в такому порядку.

- [Перші кроки](tutorial/intro.md): REPL, функції, змінні, умови, цикли, списки та `Vector`
- [Основи типів](tutorial/types.md): статичні типи, `Option`, `Result`, структури, переліки, узагальнені типи
- [Трейти](tutorial/traits.md): `deftrait` / `impl`, обмеження трейтами, `:dyn`
- [Макроси](tutorial/macros.md): `defmacro`, квазіцитування, `gensym`, `macrolet`
- [Обробка помилок](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Конкурентність](tutorial/concurrency.md): задачі, канали, `select`, `Mutex`, `thread`

## Посібники

- [Модулі та розкладання по файлах](guide/modules.md): `use`, `pub`, відповідність файлів і модулів
- [Компіляція](guide/compile.md): JIT, збирання виконуваних файлів за допомогою AOT-компіляції, дампи
- [Файловий ввід-вивід, потоки та мережа](guide/io.md): файли, шляхи, TCP / TLS / UDP, розв'язання імен
- [FFI для C](guide/ffi.md): виклик функцій C через `defffi` (разом зі зворотними викликами та структурами C через `def-c-struct`)
- [Інтеграція з редакторами](guide/editors.md): `typl-lsp` і налаштування VS Code / Emacs
- [Для програмістів на Common Lisp](guide/from-common-lisp.md): чим typelisp відрізняється від CL і як переписувати код на CL

## Довідник

- [Довідник із синтаксису](reference/syntax.md): лексика, запис типів, визначення, керівні форми, компіляція, конкурентність
- [Вбудовані функції](reference/functions/README.md): вбудовані функції, методи та стандартна бібліотека
- [Типи](reference/types.md): типи та трейти, які реалізує кожен із них
- [Повідомлення про помилки](reference/errors.md): що означають поширені помилки і як їх виправити

## Інтеграція з редакторами

Порядок налаштування описано в [посібнику з інтеграції з редакторами](guide/editors.md). Прив'язки клавіш і
налаштування кожного редактора перелічено в цих документах:

- [Emacs (typelisp-mode)](../../editor/emacs/README_uk.md)
- [VS Code](../../editor/vscode/README_uk.md)

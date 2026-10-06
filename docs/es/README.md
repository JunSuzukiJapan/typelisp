<!-- translated-from: docs/ja/README.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Documentación de typelisp (español)

typelisp es un Lisp con tipado estático. Para instalarlo y compilarlo, consulta el
[README.md](../../README.md) (en inglés) en la raíz del repositorio.

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

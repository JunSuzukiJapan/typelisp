<!-- translated-from: docs/ja/README.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Documentação do typelisp (português do Brasil)

typelisp é um Lisp com tipagem estática. Para instalá-lo e compilá-lo, consulte o
[README.md](../../README.md) (em inglês) na raiz do repositório.

## Tutorial

Se este é seu primeiro contato com o typelisp, leia nesta ordem.

- [Primeiros passos](tutorial/intro.md): o REPL, funções, variáveis, condicionais, laços, listas e `Vector`
- [Fundamentos de tipos](tutorial/types.md): tipos estáticos, `Option`, `Result`, estruturas, enumerações, genéricos
- [Traits](tutorial/traits.md): `deftrait` / `impl`, restrições de trait, `:dyn`
- [Macros](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Tratamento de erros](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concorrência](tutorial/concurrency.md): tarefas, canais, `select`, `Mutex`, `thread`

## Guias

- [Módulos e organização de arquivos](guide/modules.md): `use`, `pub`, como os arquivos correspondem aos módulos
- [Compilação](guide/compile.md): o JIT, criar executáveis com compilação AOT, dumps
- [E/S de arquivos, streams e rede](guide/io.md): arquivos, nomes de caminho, TCP / TLS / UDP, resolução de nomes
- [FFI de C](guide/ffi.md): chamar funções C com `defffi` (incluindo callbacks e structs C com `def-c-struct`)
- [Integração com editores](guide/editors.md): `typl-lsp` e a configuração do VS Code / Emacs
- [Para programadores de Common Lisp](guide/from-common-lisp.md): em que o typelisp difere do CL e como reescrever código CL

## Referência

- [Referência de sintaxe](reference/syntax.md): léxico, escrita de tipos, definições, formas de controle, compilação, concorrência
- [Funções embutidas](reference/functions/README.md): funções embutidas, métodos e a biblioteca padrão
- [Tipos](reference/types.md): os tipos e os traits que cada um implementa
- [Mensagens de erro](reference/errors.md): o que significam os erros mais comuns e como corrigi-los

## Integração com editores

Os passos de configuração estão no [guia de integração com editores](guide/editors.md). Os atalhos de
teclado e as configurações de cada editor estão descritos nestes documentos:

- [Emacs (typelisp-mode)](../../editor/emacs/README_pt-BR.md)
- [VS Code](../../editor/vscode/README_pt-BR.md)

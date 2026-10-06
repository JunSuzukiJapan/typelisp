<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

Um modo principal do Emacs para editar código-fonte typelisp (`.typl`).
A versão para VS Code está em [../vscode/](../vscode/README_pt-BR.md). As duas compartilham as
mesmas tabelas de palavras-chave e as mesmas regras de indentação, e
`cargo test --test editor_keyword_sync_test` verifica isso mecanicamente (veja o fim deste documento).

## Recursos

- Realce de sintaxe
  - Formas especiais e construções de controle (`defun` `let` `if` `match` `loop` `lambda` `setf`
    `as` `apply`, `print`/`println`/`format`, a família `pprint` etc.)
  - Nomes definidos (o `NAME` de `(defun NAME ...)` como nome de função, o de
    `(defstruct NAME ...)` como nome de tipo e o de `(defvar (NAME ...))` como nome de variável; o
    mesmo com `pub`, como em `(pub defun NAME ...)`)
  - Palavras-chave de namespaces e declarações (`pub` `module` `use` `load` `impl` `where`) e
    marcadores de lista lambda (`&rest` `&optional` `&key`)
  - Funções embutidas (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` etc.)
  - Tipos primitivos (incluindo `bignum` / `ratio`), tipos embutidos, tipos de erro embutidos
    (`ParseIntError` etc.), tipos de usuário `Capitalized` e o tipo de objeto trait `:dyn Trait`
  - **Usos de tipos definidos pelo usuário** (os nomes de `defstruct`/`defenum`/`deftrait`
    costumam ser minúsculos (`rect` `todo-item` `board`), então a regra `Capitalized` não os
    captura). Quando conectado ao `typl-lsp`, eles são coloridos a partir dos semantic tokens do
    servidor (isso também funciona com `eglot`; veja abaixo). Sem conexão, o modo recorre a coletar
    os nomes de tipo definidos no buffer
  - Literais (`true` `false`, literais numéricos (decimal / `0xff` / `1.5` / `1/3`), literais de
    caractere como `#\Space`, strings, palavras-chave como `:name`)
  - Diretivas de controle de `format` dentro de strings (`~a` `~5,'0d` `~{...~}` etc.)
  - Globais com "earmuffs" no estilo CL (`*print-pretty*` etc.)
- Comentários
  - Comentários de linha `;`
  - Comentários de bloco **aninháveis** `#| ... |#`
- Navegação por expressões S e indentação no estilo Lisp
- Índice de definições via `imenu` (funções / métodos / macros / tipos / traits / `impl` /
  variáveis / módulos)
- Comandos que executam a CLI `typl` (abaixo)

## Atalhos de teclado

| Tecla | Comando | O que faz |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Salva e executa `typl FILE` (via `compile`, então é possível pular para as linhas com erro) |
| `C-c C-z` | `typelisp-repl` | Inicia o REPL do `typl` em um buffer comint |

Defina a localização do `typl` com `typelisp-program` (padrão `"typl"`).
Os diagnósticos têm a forma `error: FILE:LINE:COL: ...`, que o `compilation-mode` sabe
interpretar, então `next-error` / `C-x \`` pula direto para o local.

## Instalação

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Arquivos `.typl` abrem em `typelisp-mode` automaticamente (o modo está registrado em
`auto-mode-alist`).

Com `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Servidor de linguagem (`typl-lsp`)

Depois de compilar o `typl-lsp`, ele pode ser usado a partir do `eglot` (embutido no Emacs 29+) ou
do `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Com `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Com `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Suporta diagnósticos (erros de sintaxe e de tipo e avisos de redefinição, enviados por
`textDocument/publishDiagnostics`), hover, ir para a definição (goto-definition), completação (`:`
está registrado como caractere de disparo) e semantic tokens. Referências entre arquivos via `use`
são resolvidas (o servidor procura para cima o `typelisp.toml` da raiz do projeto; para detalhes,
veja [Referência de sintaxe 3.11](../../docs/pt-BR/reference/syntax.md#311-arquivos-e-módulos-projetos-com-vários-arquivos)).
Edições não salvas nos buffers abertos se refletem imediatamente nos diagnósticos tanto dos
arquivos de que dependem quanto dos arquivos que dependem deles.

### Realce de nomes de tipo (semantic tokens)

Por meio de `textDocument/semanticTokens`, o servidor informa **as posições que o verificador
realmente resolveu como nomes de tipo**. Como isso não é comparação de texto:

- Tipos que vêm de outros arquivos via `use` também são coloridos (um alcance que a resolução
  dentro do buffer não consegue atingir, por princípio)
- Chamadas de uma **função** com o mesmo nome de um tipo não são coloridas (o verificador as
  resolveu como funções, então nenhum token é registrado ali)

Do lado do cliente:

- **`eglot` (Emacs 31 e posteriores)**: o eglot desenha os tokens por conta própria
  (`eglot-semantic-tokens-mode`). O `typelisp-mode` não interfere
- **`eglot` (Emacs 30 e anteriores)**: esta versão do eglot não trata semanticTokens. Por isso
  **o `typelisp-mode` envia a requisição por conta própria e desenha o resultado com overlays**
  (`typelisp-semantic-tokens-mode`, ativado automaticamente quando o eglot se conecta)
- **`lsp-mode`**: suporte nativo (defina `lsp-semantic-tokens-enable` como `t`). Nesse caso o
  `typelisp-mode` não interfere

O `scripts/emacs-semantic-smoke.el` se conecta de verdade pelo eglot e verifica a parte que faz o
desenho no Emacs em uso. Com qualquer cliente, o método alternativo dentro do buffer recua enquanto
o servidor responde (para que dois conjuntos de regras não pintem o mesmo buffer).

| Opção | Padrão | O que faz |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Com o eglot do Emacs 30 e anteriores, se deve colorir a partir dos semantic tokens do servidor |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Segundos ociosos após uma edição antes de pedir de novo (mantenha maior que `eglot-send-changes-idle-time`) |

## Observações

- typelisp converte símbolos para minúsculas ao lê-los, mas o realce diferencia maiúsculas para
  distinguir nomes de tipo que começam com letra maiúscula.
- A indentação é decidida pela função dedicada `typelisp-indent-function`, que consulta
  `typelisp-indent-specs` (uma lista de associação). O modo guarda suas próprias entradas mesmo
  para formas cujo nome compartilha com o Emacs Lisp (`defun` `let` `if` ...) porque as
  propriedades de símbolo são **globais**, e uma configuração para typelisp ali mudaria a
  indentação de outros buffers Lisp na mesma sessão. Além disso, as formas de typelisp diferem em
  formato mesmo quando compartilham o nome com o Emacs Lisp — `(defun NAME (PARAMS) RETTYPE ...)`
  tem três elementos de cabeçalho, e `if` é fixo em três elementos com um `else` obrigatório —,
  então os valores também não podem ser compartilhados.
  Foi verificado que nenhum arquivo `.typl` em `examples/` muda um único byte com `indent-region`,
  e que remover toda a indentação e reindentar restaura o original (a versão para VS Code atende ao
  mesmo critério com os mesmos arquivos).

## Detecção de divergência nas definições do editor

As tabelas de palavras-chave são mantidas em duplicidade, aqui e na versão para VS Code. Para
evitar que as definições do editor fiquem para trás enquanto a implementação avança, há um teste no
lado do Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Ele carrega de fato o prelude, percorre o registro e informa **os nomes que algum dos editores não
conhece**. Formas especiais não têm representação em tempo de execução, então são lidas de entre
`// SPECIAL-FORM DISPATCH BEGIN` / `END` em `crates/typelisp-front/src/check/checker.rs` (não apague
esses comentários). Se falhar, adicione os nomes informados a **ambas** as definições de editor.

O mesmo teste também compara a legenda de semantic tokens (`SEMANTIC_TOKEN_TYPES` em
`src/bin/lsp.rs` e as tabelas dos dois editores devem coincidir em nomes e ordem). Uma divergência
não causa erro em tempo de execução; apenas troca as cores de todos os tokens, por isso é fixada
mecanicamente.

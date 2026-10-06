<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

Uma extensão do VS Code para editar código-fonte typelisp (`.typl`).
A versão para Emacs está em [../emacs/](../emacs/README_pt-BR.md). As duas compartilham as mesmas
tabelas de palavras-chave e as mesmas regras de indentação, e
`cargo test --test editor_keyword_sync_test` verifica isso mecanicamente (veja abaixo).

## Recursos

- **Realce de sintaxe** (uma gramática TextMate; não precisa de servidor de linguagem)
  - Formas especiais e construções de controle (`defun` `let` `if` `match` `loop` `lambda` `setf`
    `as` `apply`, `print`/`println`/`format`, a família `pprint`)
  - Nomes definidos (`(defun NAME ...)` como função, `(defstruct NAME ...)` como tipo,
    `(defvar (NAME ...))` como variável; o mesmo com `pub`, como em `(pub defun NAME ...)`) e os
    dois nomes de `(impl Trait Type)`
  - Palavras-chave de namespaces e declarações (`pub` `module` `use` `load` `impl` `where`) e
    marcadores de lista lambda (`&rest` `&optional` `&key`)
  - Funções embutidas, tipos primitivos (incluindo `bignum` / `ratio`), tipos de erro embutidos,
    tipos de usuário `Capitalized` e o tipo de objeto trait `:dyn Trait` (também dentro de
    argumentos genéricos)
  - Literais numéricos (decimal / `0xff` / `1.5` / `3.0e10` / `1/3`), literais de caractere como
    `#\Space`, palavras-chave como `:name` e globais com "earmuffs" como `*print-pretty*`
  - **Diretivas de controle de `format` dentro de strings** (`~a` `~5,'0d` `~{...~}` `~^` etc.)
  - Comentários de linha `;` e comentários de bloco **aninháveis** `#| ... |#`
- **Usos de tipos definidos pelo usuário** (semantic tokens)
  - Os nomes de `defstruct` / `defenum` / `deftrait` costumam ser minúsculos (`rect` `todo-item`
    `board`), então a regra `Capitalized` não os captura, e uma gramática TextMate trabalha linha
    a linha e não enxerga o arquivo inteiro. Semantic tokens enxergam, o que resolve a situação de
    uma linguagem estaticamente tipada em que só as anotações de tipo ficavam sem cor
  - Quando conectada ao `typl-lsp`, a extensão recebe **as posições que o verificador realmente
    resolveu como nomes de tipo**. Assim, tipos que vêm de outros arquivos via `use` são coloridos,
    e chamadas de uma **função** com o mesmo nome de um tipo não (o verificador as resolveu como
    funções, então nenhum token é registrado ali)
  - Quando o servidor não está conectado ou não foi compilado, a extensão recorre a uma varredura
    de texto que resolve dentro do arquivo. É uma aproximação: não encontra tipos de outros
    arquivos e não distingue uma função com o mesmo nome de um tipo
- **Indentação Lisp** (o VS Code não tem indentação Lisp embutida, então a extensão a implementa)
  - Formatar documento, formatar seleção e formatar ao digitar (Enter e `)`, com
    `editor.formatOnType` ativado)
- **Outline / breadcrumbs / `Ctrl+Shift+O`** (funções, métodos, macros, tipos, traits, `impl`,
  variáveis, módulos)
- **Integração com o `typl-lsp`** (diagnósticos, hover, ir para a definição, completação, semantic
  tokens)
- **Comandos da CLI `typl`** (executar, REPL)

Tudo, exceto o servidor de linguagem, funciona só com a extensão, então mesmo em uma cópia em que o
`typl-lsp` não foi compilado estão disponíveis realce, indentação, Outline e realce de tipos
(restrito ao arquivo).

## Instalação

A extensão não está no Marketplace, então compile e instale localmente.

```sh
cd editor/vscode
npm install
npm run compile
```

Depois, uma destas opções:

- **Experimentar em um host de desenvolvimento**: abra `editor/vscode` no VS Code e pressione `F5`
- **Instalar permanentemente**: gere um `.vsix` com `npx @vscode/vsce package` e use
  "..." → "Install from VSIX..." na visão de extensões

Arquivos `.typl` abrem no modo typelisp automaticamente.

## Atalhos de teclado

| Tecla | Comando | O que faz |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Salva e executa `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Inicia o REPL do `typl` |

A paleta de comandos também tem `typelisp: Restart Language Server`.

## Configurações

| Configuração | Padrão | O que faz |
|---|---|---|
| `typelisp.program` | `typl` | Caminho da CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Se deve conectar ao `typl-lsp` |
| `typelisp.languageServer.path` | (vazio) | Caminho do `typl-lsp`. Se vazio, procura nesta ordem: `target/release/typl-lsp` do workspace, `target/debug/typl-lsp` e `PATH` |
| `typelisp.trace.server` | `off` | Registra o tráfego JSON-RPC do LSP |

Compile o servidor de linguagem com:

```sh
cargo build --release --bin typl-lsp
```

Referências entre arquivos via `use` são resolvidas procurando para cima o `typelisp.toml` da raiz
do projeto (para detalhes, veja
[Referência de sintaxe 3.11](../../docs/pt-BR/reference/syntax.md#311-arquivos-e-módulos-projetos-com-vários-arquivos)).

## O problem matcher de tarefas

A extensão fornece um problem matcher chamado `typelisp`. O `typl` imprime diagnósticos na forma
`error: FILE:LINE:COL: message`, então eles podem ir direto para o painel Problemas:

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

## Desenvolvimento

```sh
npm run compile   # tsc
npm run watch     # compila ao detectar mudanças
npm test          # node --test (gramática, indentação, símbolos, referências de tipo, manifesto)
```

Os testes cobrem só as partes que não precisam do módulo `vscode`. Para isso, `src/indent.ts` e
`src/symbols.ts` são escritos como funções puras, e só `src/extension.ts` toca a API do editor.

- `src/test/grammar.test.ts` — tokeniza de verdade com a gramática, usando o mesmo motor do VS
  Code (`vscode-textmate` + `vscode-oniguruma`), e verifica o resultado.
  O Oniguruma difere das expressões regulares do Emacs em detalhes (por exemplo, não trata como
  literal um `]` no início de uma classe de caracteres), e essas diferenças só aparecem rodando o
  motor real.
- `src/test/indent.test.ts` — para cada arquivo `.typl` em `examples/`, exige que **remover toda a
  indentação e restaurá-la coincida byte a byte com o conteúdo commitado**.
  O modo do Emacs atende ao mesmo critério com os mesmos arquivos, e é isso que torna "os dois
  editores concordam" uma afirmação verificada.
  Além disso, `src/test/fixtures/emacs-indent-reference.txt` é uma saída de referência coletada
  rodando de fato `indent-region` em um buffer `typelisp-mode` do Emacs. O valor esperado não é uma
  repetição da implementação em TS, mas **o que o outro editor realmente produz**, então a
  fidelidade da portabilidade é verificada diretamente (inclui `let*` `do` `doiter` `labels`
  `impl` `pprint-logical-block`, prefixos de quote e mais).
- `src/test/symbols.test.ts` — o conteúdo do Outline e a detecção de referências de tipo do método
  alternativo. O número de definições deve coincidir exatamente com uma contagem independente das
  formas de definição no início das linhas. As regras de fronteira das referências de tipo foram
  alinhadas de propósito com o método alternativo da versão para Emacs (o VS Code usa lookbehind;
  o Emacs expressa o mesmo conjunto consumindo um caractere anterior).
- Os tokens do servidor guiados pela resolução (`crates/typelisp-front/src/check/semantic.rs`) são
  verificados por `cargo test --test lsp_semantic_test` e `scripts/lsp-semantic-smoke.py` (que
  conduz um processo real via stdio). O cliente do Emacs é verificado por
  `scripts/emacs-semantic-smoke.el` com uma conexão real do eglot.
- `src/test/manifest.test.ts` — o `package.json` é a única parte que o compilador não verifica,
  então este teste confere que os comandos declarados e as chamadas de `registerCommand` formam o
  mesmo conjunto, a que os atalhos de teclado se referem, que as configurações lidas pelo código
  estão declaradas e que o problem matcher consegue interpretar o que o `typl` realmente imprime.

### Detecção de divergência nas definições do editor

As tabelas de palavras-chave são mantidas em duplicidade, na versão para Emacs e na versão para VS
Code. Para evitar que as definições do editor fiquem para trás enquanto a implementação avança, há
um teste no lado do Rust:

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

## Observações

- typelisp converte símbolos para minúsculas ao lê-los, mas o realce diferencia maiúsculas para
  distinguir nomes de tipo que começam com letra maiúscula.
- A indentação é decidida por `INDENT_SPECS` em `src/indent.ts`. É uma portabilidade de
  `typelisp-indent-specs` da versão para Emacs, com os mesmos valores e regras. Os pontos em que uma
  forma difere em formato da forma homônima do Emacs Lisp são mantidos como estão: o cabeçalho de
  `(defun NAME (PARAMS) RETTYPE ...)` tem três elementos, `if` é fixo em três elementos com um
  `else` obrigatório etc.
- O conteúdo de `#| ... |#` é reindentado ao formatar. Isso coincide com o comportamento de
  `indent-region` do Emacs.

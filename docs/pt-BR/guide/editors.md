<!-- translated-from: docs/ja/guide/editors.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Integração com editores (typl-lsp)

`typl-lsp` é o servidor de linguagem do typelisp. Conectado a um editor com suporte a LSP (o Language Server
Protocol), ele oferece estes recursos para o arquivo que você está editando:

- Diagnósticos: erros de leitura, erros de tipo e avisos de redefinição
- Informação ao passar o mouse: o tipo de uma expressão entre parênteses e a docstring da definição que ela
  chama (não é mostrada para nomes de variável simples)
- Ir para a definição
- Autocompletar (os candidatos aparecem quando você digita `:`)
- Coloração dos nomes de tipo (tokens semânticos), incluindo tipos trazidos com `use` de outros arquivos

As referências entre arquivos via `use` são resolvidas. Edições não salvas em outro arquivo aberto se
refletem imediatamente nos diagnósticos dos arquivos que o usam com `use`.

## 1. Compilação

```sh
cargo build --release --bin typl-lsp
```

Isso produz `target/release/typl-lsp`. Se você instalou com `cargo install` como descrito no
[README.md](../../../README.md), ele está em `~/.cargo/bin/typl-lsp` junto com o `typl`.

## 2. VS Code

A extensão está em `editor/vscode` no repositório. Ela não está publicada no Marketplace, então compile-a e
instale-a você mesmo.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produz um .vsix
```

Escolha "Install from VSIX..." no menu "..." da visão de extensões e selecione o `.vsix` que você compilou.

A extensão procura `typl-lsp` em `target/release/typl-lsp` do workspace, depois em `target/debug/typl-lsp`,
depois no `PATH`. Se você o colocar em outro lugar, escreva o caminho na configuração
`typelisp.languageServer.path`.

| Configuração | Padrão | Significado |
|---|---|---|
| `typelisp.program` | `typl` | Caminho do `typl` |
| `typelisp.languageServer.enable` | `true` | Se conecta ao `typl-lsp` |
| `typelisp.languageServer.path` | (vazio) | Caminho do `typl-lsp` |

`Ctrl+Alt+R` salva o arquivo que você está editando e o executa com o `typl`, e `Ctrl+Alt+Z` inicia o REPL.
Para mais, consulte o [README da extensão do VS Code](../../../editor/vscode/README_pt-BR.md).

## 3. Emacs

`typelisp-mode` está em `editor/emacs` no repositório.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Configuração para conectar ao `typl-lsp` com o `eglot` (incluído no Emacs 29 e posteriores):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Com o `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

O eglot do Emacs 31 e posteriores colore por conta própria os nomes de tipo (tokens semânticos). O eglot do
Emacs 30 e anteriores não os suporta, então o `typelisp-mode` colore os nomes de tipo no lugar dele. Com o
`lsp-mode`, defina `lsp-semantic-tokens-enable` como `t`.

`C-c C-c` executa o arquivo que você está editando, e `C-c C-z` inicia o REPL. Para mais, consulte o
[README do typelisp-mode](../../../editor/emacs/README_pt-BR.md).

## 4. Outros editores

O `typl-lsp` fala LSP pela entrada e saída padrão e não recebe argumentos de linha de comando. Configure o
cliente LSP do seu editor para iniciar o `typl-lsp` para arquivos `.typl`.

## 5. Como os projetos são reconhecidos

O `typl-lsp` procura `typelisp.toml` a partir do diretório do arquivo aberto, subindo, e resolve `use` tomando
esse lugar como raiz dos fontes. São as mesmas regras de quando o `typl` executa um arquivo
([Módulos e organização de arquivos](modules.md#2-preparar-um-projeto)). Para um projeto com vários arquivos,
coloque `typelisp.toml` na raiz.

## 6. O servidor de linguagem não executa seu programa

O `typl-lsp` produz os diagnósticos apenas lendo e verificando tipos. Ele nunca executa o programa que você
está editando. Os diagnósticos rodam a cada tecla, então não dá para executar ali código com efeitos
colaterais ou código que nunca termina. A única exceção é o registro dos `defmacro`, necessário para verificar
as chamadas de macro que vêm depois deles.

Por isso, erros que só acontecem quando o `typl` executa o programa (`panic`, um arquivo que não existe etc.)
não aparecem nos diagnósticos do servidor de linguagem.

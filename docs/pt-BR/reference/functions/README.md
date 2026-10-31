<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Funções embutidas

A lista de funções embutidas, métodos e da biblioteca padrão. Para a sintaxe (formas especiais e como
definir coisas), consulte a [Referência de sintaxe](../syntax.md); para a lista de tipos, [Tipos](../types.md).

## Formas de chamada

Há três formas de chamada.

- Funções livres: `(name args...)`
- Métodos de instância: `(name receiver args...)` (resolvidos a partir do tipo estático do primeiro
  argumento)
- Métodos estáticos (funções associadas): `(Type::name args...)`

Cada tipo pode ter seu próprio método com o mesmo nome. `(+ a b)` chama o `+` do tipo de `a`.

## Nomes que precisam de um `use`

Dos nomes da biblioteca padrão, estes podem ser usados em qualquer módulo sem escrever nada
([referência de sintaxe 3.10](../syntax.md#310-module--use--namespaces)):

- os tipos primitivos, `Option`/`Result`/`Vector`, a iteração, as expressões S e as listas, as funções
  de sequência, as macros de controle, a comparação e os traits aritméticos
- todo nome que também é um nome do pacote `COMMON-LISP` de Common Lisp (`length`, `format`, `open`
  etc.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, os traits de stream `Stream`,
  `InputStream` e `OutputStream`, e `Hash`, que as chaves de uma `HashTable` implementam
- os tipos que as funções e variáveis destes mencionam

Qualquer outro nome é trazido com um `use`, como `(use std::net::tcp-connect)`, ou escrito pelo
caminho, `std::net::tcp-connect`. A tabela dá esses nomes e o módulo de cada um.

| Módulo | Nomes |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Como ler as tabelas

As tabelas de cada capítulo têm as colunas "nome, forma, tipo, descrição". A coluna de tipo é escrita como
`(tipo-do-argumento,...)→tipo-de-retorno`.

- Uma única letra maiúscula como `T`, `A` ou `B` é uma variável de tipo.
- Uma nota como `where Eq A` é uma restrição de trait que a variável de tipo deve satisfazer.
- `Iter<A>` significa "qualquer implementação de `Iter` cujo `Item` seja `A`".
- Os argumentos marcados com `&optional` / `&key` podem ser omitidos.

## Capítulos

| Arquivo | Conteúdo |
|---|---|
| [numbers.md](numbers.md) | Inteiros, números de ponto flutuante, racionais, números complexos, booleanos, operações de bits, números aleatórios |
| [sequences.md](sequences.md) | O par `cons-cell`, os dados de expressões S `Sexpr`, símbolos, funções de sequência, iteradores preguiçosos `lazy`, funções de ordem superior |
| [collections.md](collections.md) | Strings, caracteres, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, tipos de erro e o trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, traits aritméticos, `Push`, `Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`, o pretty printer, `print-object`, variáveis de controle da impressora |
| [format.md](format.md) | Diretivas de formato |
| [streams-files.md](streams-files.md) | Streams, operações com arquivos, nomes de caminho, readtable |
| [concurrency.md](concurrency.md) | Tarefas, canais, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, sockets de domínio Unix, UDP |
| [system.md](system.md) | Tempo, o ambiente de execução, ferramentas da implementação, `read`/`eval`, docstrings, funções relacionadas a macros |

<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, traits aritméticos |
| [printing.md](printing.md) | `print`/`println`/`format`, o pretty printer, `print-object`, variáveis de controle da impressora |
| [format.md](format.md) | Diretivas de formato |
| [streams-files.md](streams-files.md) | Streams, operações com arquivos, nomes de caminho, readtable |
| [concurrency.md](concurrency.md) | Tarefas, canais, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, sockets de domínio Unix, UDP |
| [system.md](system.md) | Tempo, o ambiente de execução, ferramentas da implementação, `read`/`eval`, docstrings, funções relacionadas a macros |

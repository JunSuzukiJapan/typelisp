<!-- translated-from: docs/ja/reference/types.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# Tipos

Os tipos que o typelisp tem e os traits padrão que cada tipo implementa. Como escrever tipos está no
[capítulo 2 da Referência de sintaxe](syntax.md#2-escrita-de-tipos); as funções e métodos de cada tipo, em
[Funções embutidas](functions/README.md).

## 1. Tipos primitivos

| Tipo | Conteúdo | Detalhes |
|---|---|---|
| `int` | Um inteiro de precisão arbitrária. Guardado como valor imediato enquanto cabe em 63 bits, vira bignum automaticamente além disso. O tipo padrão dos literais inteiros sem anotação | [Números, capítulo 3](functions/numbers.md#3-inteiros-de-precisão-arbitrária-int) |
| `i8` `i16` `i32` | Inteiros de largura fixa com sinal | [Números, capítulo 1](functions/numbers.md#1-inteiros-de-largura-fixa) |
| `u8` `u16` `u32` | Inteiros de largura fixa sem sinal | Igual ao acima |
| `f32` `f64` | Números de ponto flutuante IEEE-754. Os literais decimais são `f64` por padrão | [Números, capítulo 4](functions/numbers.md#4-números-de-ponto-flutuante-f64--f32) |
| `ratio` | Um número racional na forma irredutível | [Números, capítulo 5](functions/numbers.md#5-racionais-ratio) |
| `bool` | `true` / `false` | [Números, capítulo 7](functions/numbers.md#7-booleanos) |
| `char` | Um valor escalar Unicode | [Caracteres](functions/collections.md#2-caracteres-char) |
| `string` | Uma string imutável | [Strings](functions/collections.md#1-strings-string) |
| `symbol` | Um símbolo. Palavras-chave (`:name`) também têm esse tipo | [Símbolos](functions/sequences.md#3-símbolos) |
| `()` | O tipo Unit. Seu valor também é `()` | |
| `!` | O tipo Never. O tipo das expressões que não retornam, como `panic`. Pode ser colocado onde se espera qualquer tipo | |
| `ptr` `c-long` `c-ulong` | Palavras usadas só para passar valores de e para C. Só podem ser valores dentro de `unsafe`, e os lugares onde podem aparecer são limitados | [Números, capítulo 2](functions/numbers.md#2-palavras-brutas-na-fronteira-com-c-ptr--c-long--c-ulong) |
| `random-state` | O estado de um gerador de números aleatórios | [Números, capítulo 12](functions/numbers.md#12-números-aleatórios) |

Não há tipo inteiro de 64 bits. Para inteiros cuja largura não importa, use `int`.

## 2. Tipos genéricos embutidos

| Tipo | Conteúdo | Detalhes |
|---|---|---|
| `Option<T>` | Um valor que existe ou não. `some` / `none` | [Option e Result](functions/option-result.md) |
| `Result<T,E>` | Sucesso ou falha. `ok` / `err` | Igual ao acima |
| `Vector<T>` | Um array que pode crescer | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Uma tabela hash. O tipo da chave deve implementar `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | Tupla (de 1 a 12 elementos). Os elementos são lidos com `t::0` | [Sintaxe, capítulo 2](syntax.md#2-escrita-de-tipos) |
| `Task<T>` | Um handle de uma tarefa | [Tarefas](functions/concurrency.md#1-taskt--handles-de-tarefas) |
| `Thread<T>` | Um handle de uma tarefa executada em uma thread de SO dedicada | [Thread](functions/concurrency.md#7-threadt--threads-de-so-dedicadas) |
| `Chan<T>` | Um canal | [Canais](functions/concurrency.md#2-chant--canais) |

Os tipos de função são escritos `(fn (tipos-dos-argumentos...) tipo-de-retorno)`, e os objetos trait
`:dyn Trait` ([capítulo 2 da Referência de sintaxe](syntax.md#2-escrita-de-tipos)).

## 3. Dados de expressões S

| Tipo | Conteúdo | Detalhes |
|---|---|---|
| `Sexpr` | Uma expressão S não vazia. 19 variantes: `int`, `i8` a `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [Dados de expressões S](functions/sequences.md#2-dados-de-expressões-s-sexpr) |
| `Option<Sexpr>` | Os dados de expressões S em geral. A lista vazia `()` é `none` | Igual ao acima |

## 4. Tipos da biblioteca padrão

Tipos que a biblioteca padrão (o prelude) define com `defstruct` / `defenum`. Eles são tratados como os tipos
que você mesmo escreve, e tudo o que se pode fazer com um `defstruct` pode ser feito com eles.

| Tipo | Conteúdo | Detalhes |
|---|---|---|
| `cons-cell<A,B>` | Um par. `cons`/`car`/`cdr` | [Pares](functions/sequences.md#1-pares-cons-cellab) |
| `complex` | Um número complexo (componentes `f64`) | [Números, capítulo 6](functions/numbers.md#6-números-complexos-complex) |
| `Array<T>` | Um array multidimensional | [Array](functions/collections.md#5-arrayt-arrays-multidimensionais) |
| `BitVector` | Uma sequência de bits de comprimento fixo | [BitVector](functions/collections.md#6-bitvector-vetores-de-bits) |
| `HashSet<T>` | Uma coleção de elementos sem duplicatas | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | Uma tabela mantida em ordem de chave | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | Uma sequência em que se põe e se tira pelas duas pontas | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Os iteradores devolvidos por `iter` de cada coleção | [Iter](functions/traits.md#1-o-trait-iter-e-a-iteração) |
| `lazy::map-iter<I,A,U>` etc. | Os iteradores que as funções do módulo `lazy` devolvem | [Iteradores preguiçosos](functions/sequences.md#iteradores-preguiçosos-o-módulo-lazy) |
| `WaitGroup` | Esperar que N coisas terminem | [WaitGroup](functions/concurrency.md#4-waitgroup--esperar-n-conclusões) |
| `Mutex<T>` | Exclusão mútua para dados compartilhados | [Mutex](functions/concurrency.md#6-mutext--exclusão-mútua-para-dados-compartilhados) |
| `pathname` | Um nome de arquivo dividido em partes | [Nomes de caminho](functions/streams-files.md#9-nomes-de-caminho-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Streams | [Streams](functions/streams-files.md#3-tipos-de-stream-concretos) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Streams compostos | [Streams compostos](functions/streams-files.md#4-streams-compostos) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Rede | [Rede](functions/network.md#1-tipos) |
| `ReadOutcome` | O resultado de `read-sexpr`. `datum` / `eof` | [Streams](functions/streams-files.md#6-funções-genéricas-e-operações-com-arquivos) |
| `universal-time` `internal-time` `decoded-time` | Tempo | [Tempo](functions/system.md#1-tempo) |
| `heap-info` | O estado atual do heap | [Ferramentas da implementação](functions/system.md#51-campos-de-heap-info) |

## 5. Tipos de erro

`Error` não é um tipo, mas um trait, e os tipos a seguir o implementam. Para lidar com erros de qualquer tipo,
escreva `:dyn Error`.

| Tipo | Produzido por |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operações com arquivos e streams |
| `NetError` | Operações de rede |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Os detalhes estão em [Tipos de erro e o trait Error](functions/option-result.md#3-tipos-de-erro-e-o-trait-error).

## 6. Implementações dos traits padrão

Quais tipos implementam quais traits. Os métodos de cada trait estão em [Traits padrão](functions/traits.md)
e nos capítulos indicados na coluna da direita.

### 6.1 Comparação, hash e impressão

| Trait | Tipos que o implementam | Detalhes |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-comparação) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | Igual ao acima |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `pathname` `universal-time` `internal-time` e todos os tipos de erro embutidos | [print-object](functions/printing.md#5-print-object-representação-impressa-por-tipo) |

Os traits de `cons-cell<A,B>` e das tuplas `#{..}`, e o `print-object` das coleções, podem ser
usados quando os tipos dos elementos implementam esse trait.

### 6.2 Aritmética

| Trait | Tipos que o implementam |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Os detalhes estão em [Traits aritméticos](functions/traits.md#3-traits-aritméticos-add--sub--mul--div--rem--bits--number).

### 6.3 Iteração

| Trait | Tipos que o implementam |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` os tipos do módulo `lazy` (`lazy::map-iter<I,A,U>` etc.) |

### 6.4 Streams

| Tipo | Traits implementados |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Todo stream implementa `Stream`; os streams de entrada também implementam `InputStream`, e os de saída
`OutputStream`. `socket-listener` e `udp-socket` implementam só `Stream` (`close` / `open-stream-p`). Os
detalhes estão em [Streams](functions/streams-files.md#1-a-hierarquia-de-traits).

### 6.5 Outros

| Trait | Tipos que o implementam | Detalhes |
|---|---|---|
| `Error` | Todos os tipos de erro do capítulo 5 | [Tipos de erro](functions/option-result.md#3-tipos-de-erro-e-o-trait-error) |
| `Pathish` | `string` `pathname` | [Nomes de caminho](functions/streams-files.md#91-o-trait-designador-de-caminhos-pathish) |

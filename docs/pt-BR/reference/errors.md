<!-- translated-from: docs/ja/reference/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Mensagens de erro

O que significam as principais mensagens de erro do `typl` e como corrigi-las.

## 1. Como ler um erro

Os erros são escritos na saída de erro padrão nesta forma:

```text
error: arquivo:linha:coluna: tipo: mensagem
```

O `tipo` diz quando o erro foi encontrado.

| Tipo | Quando | Significado |
|---|---|---|
| `type error` | Antes da execução (na verificação) | Um erro de tipos ou de nomes. Essa forma não é executada |
| (sem tipo) | Na leitura ou na verificação | Um erro de sintaxe como parênteses desbalanceados, ou um nome que não é encontrado |
| `panic` | Durante a execução | Uma falha irrecuperável. O programa para depois de executar a limpeza de `unwind-protect` |

As linhas que começam com `warning:` são avisos, e o processamento continua.

`arquivo:linha:coluna` aponta para a expressão com o erro. Para um erro em tempo de execução que acontece
dentro de uma função da biblioteca padrão, aponta para o lugar onde o programa chamou essa função. Alguns
erros não têm posição (como `error: panic: ...`).

Exemplo:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Significa que a expressão da linha 1, coluna 24 de `main.typl` era uma `string` onde se esperava um `i32`.

## 2. Erros na verificação

Erros encontrados antes da execução. A forma não é executada até que sejam corrigidos.

### 2.1 Tipos

| Mensagem | Significado e correção |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Há uma expressão do tipo `U` onde se precisa do tipo `T`. Não há conversões implícitas; para números, converta com `(as T x)`. `int` e `i32` também são tipos diferentes |
| ``integer literal 300 is out of range for u8 (0..=255)`` | O literal não cabe no tipo. Se quiser recortá-lo, escreva `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Não há tipo com esse nome. Defina o tipo antes da primeira forma que o usa (tipos não têm declaração antecipada). Se você quis dizer uma variável de tipo, escreva-a em uma posição de declaração como `<foo>` depois do nome da função ([Referência de sintaxe 3.6](syntax.md#36-defstruct--estruturas-tipos-definidos-pelo-usuário)) |
| ``cannot infer type argument `t` for `vector::new` `` | Não é possível determinar um argumento de tipo. Escreva o tipo com `the`, como em `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | O `match` não trata todas as variantes. Adicione ramos para as variantes que faltam, ou um ramo `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | A função exige um trait que o tipo que você passou não implementa. Escreva `(impl Eq pt ...)` ([Traits padrão](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Um valor de um tipo que não implementa o trait foi passado onde se espera um `:dyn`. Escreva o `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Um nome de trait foi escrito onde vai um tipo. Escreva `:dyn Error` |
| ``if: (if cond then else)`` | O `if` tem a forma errada. `if` exige um ramo else. Quando não precisar dele, use `when` |

### 2.2 Nomes

| Mensagem | Significado e correção |
|---|---|
| `no such function: bar` | Não há função nem método com esse nome. Confira a grafia |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Os métodos são selecionados pelo tipo do primeiro argumento. Existe um método com esse nome, mas não para o tipo do primeiro argumento (`int` aqui). O fim da mensagem lista os tipos que têm o método |
| `unbound variable: y` | Não há variável com esse nome. Confira a grafia e o escopo do vínculo (ela está sendo usada fora do seu `let`?) |
| ``use: unresolved `nosuch` `` | O módulo indicado em `use` não foi encontrado. Para saber como os nomes de arquivo correspondem aos caminhos de módulo, consulte [Referência de sintaxe 3.11](syntax.md#311-arquivos-e-módulos-projetos-com-vários-arquivos) |
| `unresolved path: c::hidden` | O módulo existe, mas o nome não, ou não é visível porque falta `pub` |
| `circular module dependency: a -> b -> a` | Os módulos se usam mutuamente com `use`. Mova a parte compartilhada para um módulo separado |
| ``return-from: no enclosing block named `nope` `` | Nenhum `block` com o nome dado a `return-from` o envolve. O bloco de uma função só pode ser usado dentro dessa função |

### 2.3 Chamadas

| Mensagem | Significado e correção |
|---|---|
| `f: expected 1 argument(s), got 2` | O número de argumentos não bate |
| `f: unknown keyword argument :b` | Foi passado um argumento de palavra-chave que a função não tem |
| `new: expected 1 field(s), got 2` | O número de valores passados ao construtor de uma estrutura não bate com o número de campos |
| ``setf: cannot assign to constant `k` `` | Houve atribuição a um nome definido com `defconstant`. Se ele precisa mudar, use `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Uma função declarada com `defsignature` não está definida |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Nenhum dos tipos dos argumentos tem o método chamado com `~/name/` ([Diretivas de formato, capítulo 5](functions/format.md#5-name)) |

## 3. Erros de leitura

| Mensagem | Significado e correção |
|---|---|
| `unexpected end of input while reading a list` | Falta um parêntese de fechamento. A posição aponta onde a leitura terminou (como o fim do arquivo), então procure o parêntese de abertura |

## 4. Erros em tempo de execução (panic)

| Mensagem | Significado e correção |
|---|---|
| `panic: divide by zero` | Divisão por zero com inteiros ou racionais. A divisão por zero em ponto flutuante não causa panic; dá `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` foi aplicado a `none`. Trate o caso `none` com `match` ou `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Um índice fora do intervalo. Verifique o comprimento com `len`, ou use uma função que devolva `none` fora do intervalo (`nth`, `pop` etc.) |
| `panic: an integer argument does not fit a fixnum` | Um `int` que não cabe em 63 bits foi passado a um argumento que recebe um índice ou uma contagem |
| `throw: no enclosing (catch 'oops) for this throw` | Um `throw` foi executado sem nenhum `catch` envolvente com a mesma etiqueta |
| `panic: <message>` | O programa chamou `(panic "<message>")`. Um `assert` que falha dá `assertion failed: ...` |

Um `panic` para o processo inteiro mesmo quando acontece dentro de uma tarefa
([Referência de sintaxe 12.4](syntax.md#124-interação-com-outros-recursos)). Expresse com `Result` as falhas
das quais você quer se recuperar ([capítulo 9 da Referência de sintaxe](syntax.md#9-política-de-tratamento-de-erros)).

## 5. Avisos

| Mensagem | Significado |
|---|---|
| ``warning: redefining function `f` `` | Uma função com o mesmo nome foi definida de novo. A definição posterior passa a valer. Aparece normalmente quando você corrige uma definição no REPL |

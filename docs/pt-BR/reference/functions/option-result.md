<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result e tipos de erro

## 1. `Option<T>` / `Result<T,E>`

Construtores: `Option<T>` tem `Some(T)` / `None`. `Result<T,E>` tem `Ok(T)` / `Err(E)`. `E` pode ser qualquer
tipo: os tipos de erro concretos embutidos e os tipos que você mesmo escreve com `defstruct`/`defenum` cabem
ali igualmente (capítulo 3).

| Nome | Forma | Option | Result | Descrição |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Tira o valor. Panic com `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | O valor, ou o valor padrão |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Se é `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Se é `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Se é `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Se é `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Extrai o valor. Com `None`/`Err`, entra em panic com `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | O valor, ou o resultado de `f`. `f` só é chamada com `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Aplica `f` ao conteúdo de `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Aplica `f` ao conteúdo de `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Com `Some`/`Ok`, passa o conteúdo a `f` e devolve o resultado dela |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Com `None`/`Err`, devolve o resultado de `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Transforma `Some(v)` em `Ok(v)` e `None` em `Err(e)` |

Os construtores são `Option::some`/`Option::none`/`Result::ok`/`Result::err` (ou, depois de
`(use option)`/`(use result)`, os nomes simples `some`/`none`/`ok`/`err`).

A ramificação é escrita explicitamente com `match`, ou encadeada com `map`/`and-then` e os demais
acima. Não há sintaxe correspondente ao `?` do Rust.

O `map` de `Option`/`Result` é um método, diferente do `map` das [sequências](sequences.md). É ele
que é chamado quando o tipo do primeiro argumento é `Option`/`Result`.

A macro `->` passa um valor como primeiro argumento de cada forma seguinte, em ordem (como o `->` do
Clojure). `(-> x (f a) (g b))` vira `(g (f x a) b)`. Um nome sem parênteses, `h`, é tratado como
`(h x)`. O primeiro argumento de um método é o seu receptor, então os combinadores se encadeiam como
estão:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. A representação em tempo de execução de `Option<T>`

Como no Rust, **`Option<T>` normalmente não cria uma caixa**. `some v` é o próprio `v` e `none` é o valor da
lista vazia, sem alocação nem indireção. `Option<Sexpr>` (em que a lista vazia é `none`), `Option<int>`,
`Option<string>`, `Option<my-struct>`, `Option<f64>` e `Option<(fn ...)>` tomam todos essa forma.

Uma caixa só é usada quando um valor de `T` não pode ser distinguido do valor da lista vazia:

| `T` | Representação | Motivo |
|---|---|---|
| `Option<U>` (aninhado) | Caixa | O `none` interno seria o mesmo valor que o `none` externo |
| `()` | Caixa | O valor de `()` é o próprio valor da lista vazia |
| `ptr` / `c-long` / `c-ulong` | Caixa | Todos os 64 bits são valor, sem espaço para distingui-los |
| Qualquer outro | Sem caixa | — |

A representação é decidida só pelo tipo e não pode ser lida a partir de um valor. Ao imprimir,
`(some ...)`/`none` é reconstruído a partir do tipo estático, então `(format false "~a" opt)` imprime
`(some 1)`. Há duas restrições:

- **Não pode ser colocado em um `:dyn Trait`** (passar um valor de `Option<int>` para o qual você escreveu
  `(impl Speak Option<int> ...)` a um `:dyn Speak` é um erro).
- Uma conversão descendente `(the Option<T> ...)` a partir de um `Sexpr` **nomeia um construtor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. A forma que vincula o valor inteiro,
  `(the Option<int> o)`, é um erro.

## 3. Tipos de erro e o trait `Error`

Seguindo o `std::error::Error` do Rust, **`Error` não é um tipo, mas um trait**. Os tipos concretos que
representam erros são separados para cada propósito, e cada um implementa `Error`.

| Tipo | Produzido por |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operações com arquivos e streams ([Streams e arquivos](streams-files.md)) |
| `NetError` | Operações de rede ([Rede](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. O `simple-error` do CL: a escolha padrão quando você só quer dizer o que aconteceu |
| `WrappedError` | `(wrap-error msg cause)`. Um tipo que carrega ao mesmo tempo sua própria mensagem e a causa; é o motivo de o trait `Error` ter `source` |

De `ParseIntError` a `NetError`, cada um é "uma enumeração com uma única variante que contém uma string de
mensagem", e o nome do tipo e o nome da variante são iguais (`(match e ((ParseIntError m) m))`, construído com
`(ParseIntError::ParseIntError "...")`). Não há nada de especial neles: são tratados exatamente como os seus
próprios tipos de erro escritos com `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | A mensagem de erro (um método do trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | A causa que este erro envolve, ou `None` se não houver (o `Error::source` do Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementa `Error`) | Amplia um tipo de erro concreto para o objeto trait |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementa `Error`) | A mensagem e a cadeia de causas encontradas seguindo `source`, uma causa por linha. O CL não tem equivalente (o "caused by" do Rust) |

Se você implementar `Error` para seu próprio tipo de erro, ele pode ser tratado **do mesmo jeito** que os erros
embutidos:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; o tipo concreto vai como está em E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; tratar qualquer tipo de modo uniforme
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Para reunir vários tipos de erro em um único `Result`, use `Result<T, :dyn Error>` (corresponde ao
`Box<dyn Error>` do Rust), e amplie os erros concretos com `as-dyn-error`. Como não há `?`, essa conversão é
escrita explicitamente:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Tipos e traits compartilham um namespace** (como no Rust). Dentro de um módulo, um `defstruct`/`defenum` e um
trait não podem ter o mesmo nome, e escrever um nome de trait em uma posição de tipo é relatado como
"`error` is a trait, not a type — write `:dyn error`".

As falhas irrecuperáveis são expressas com `panic`. Para `panic` e `catch`/`throw`, consulte a
[Referência de sintaxe](../syntax.md#8-saídas-não-locais-catch--throw--unwind-protect); para a política de
tratamento de erros, [o capítulo 9 do mesmo documento](../syntax.md#9-política-de-tratamento-de-erros).

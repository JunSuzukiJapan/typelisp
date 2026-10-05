<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Tratamento de erros

O tratamento de erros no typelisp divide as falhas em dois tipos.

| Tipo de falha | Exemplos | Como é expressa |
|---|---|---|
| Falhas que podem acontecer (recuperáveis) | Um arquivo não existe, a entrada não é um número | Devolver um `Result<T,E>` |
| Erros no programa (irrecuperáveis) | Um índice fora do intervalo, `unwrap` de `none`, divisão por zero | Parar com `panic` |

Além disso existem `catch` / `throw`, que saem de muitas chamadas de função de uma vez, e `unwind-protect`,
que executa uma limpeza qualquer que seja a forma de sair do corpo. Este capítulo pressupõe que você leu a
seção sobre `Result` de [Fundamentos de tipos](types.md).

## 1. Devolver um `Result` e recebê-lo com `match`

Esta é uma função que lê um número de porta de uma string. Ela pode falhar de duas formas: a entrada não é um
número, ou está fora do intervalo.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Quem chama separa o sucesso da falha com `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- O valor de uma função que devolve `Result` não pode ser usado a menos que `match` trate o caso `err`.
  Esquecer de tratar a falha é um erro de tipo.
- O erro de `parse-int` é um valor do tipo `ParseIntError`. `(message e)` dá a string da mensagem.

## 2. Repassar uma falha para quem chama

Não existe uma forma abreviada como o `?` do Rust. Ao chamar em sequência várias funções que devolvem
`Result`, a parte de "devolver a falha como está" é escrita com `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Quando você sabe que uma operação não pode falhar, ou em um script pequeno em que parar na falha não tem
problema, `unwrap` tira o conteúdo. Se o valor for um `err`, ocorre um panic. Se um valor padrão bastar, use
`unwrap-or`.

## 3. Criar seu próprio tipo de erro

Expressar os erros como um tipo em vez de uma string permite que quem chama ramifique conforme o tipo de
erro. Um tipo de erro é um `defenum` ou `defstruct` comum que implementa o trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; falta uma configuração
  (invalid string int))     ; um valor está errado

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` devolve uma descrição do erro.
- `source` devolve outro erro que causou este. Sem causa, é `none`.

## 4. Combinar tipos diferentes de erros

Se uma função chama tanto `parse-int` (`ParseIntError`) quanto `check-workers` (`config-error`), há dois tipos
de erro, e eles não podem ser ambos o `E` de um mesmo `Result<T,E>`. Nesse caso, faça `E` ser `:dyn Error` (um
erro de qualquer tipo que implemente `Error`). Converta cada erro com `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Dados `"4"`, `"-1"` e `"abc"`, os resultados são:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Para `:dyn`, consulte a seção 5 de [Traits](traits.md).

## 5. `panic`: erros no programa

Quando o programa chega a um estado que nunca deveria acontecer, pare-o com `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- O tipo de `panic` é `!` (não retorna), então pode ser escrito onde quer que se espere qualquer tipo. É por
  isso que os dois ramos do `if` acima se encaixam.
- Estas operações também causam panic: `unwrap` de `none` ou `err`, `get` com índice fora do intervalo e
  divisão inteira por zero.
- `panic` para o programa. Mesmo quando acontece dentro de uma tarefa, o programa inteiro para.
- No REPL, um `panic` não encerra o REPL; ele espera a próxima entrada.
- Você pode escrever `(todo)` para "ainda não escrito" e `(unreachable)` para "nunca se deveria chegar aqui".
  Ambos causam panic.

`panic` não substitui `Result`. Para falhas que podem acontecer, como a entrada do usuário ou a existência
de um arquivo, use `Result`.

## 6. `catch` / `throw`: saltar através de funções

`throw` salta diretamente para o `catch` envolvente com a mesma etiqueta, quantas chamadas de função houver
no meio.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Se `v` não tem nenhum número negativo, `validate` devolve `"all fine"`; se contém `-7`, o controle salta de
dentro de `check-all` para o `catch`, que devolve `"negative: -7"`.

- Escreva a etiqueta como um símbolo simples, como `'bad-input`.
- **Cada etiqueta carrega valores de um único tipo.** No exemplo acima `'bad-input` carrega uma `string`,
  então lançar um `int` com a mesma etiqueta é um erro de tipo. O tipo do corpo do `catch` também precisa
  bater com o tipo da etiqueta.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Um `throw` sem nenhum `catch` com a mesma etiqueta para alcançar é um erro.

Se você só quer retornar mais cedo dentro de uma função, use `return-from` em vez de `catch` / `throw`.
`return-from` não pode atravessar funções, mas em troca dá para ver para onde ele retorna lendo o código-fonte.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: limpar sempre

`(unwind-protect corpo limpeza)` executa a limpeza qualquer que seja a forma de sair do corpo: quando termina
normalmente, quando se sai por `throw` e quando ocorre um panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Use-o para coisas como "sempre fechar um arquivo que você abriu" ou "sempre liberar uma trava que você
pegou". `with-open-file` e `with-lock` da biblioteca padrão usam `unwind-protect` internamente.

## 8. Sobre o sistema de condições do Common Lisp

typelisp não adota o sistema de condições do Common Lisp (`handler-case`, `restart-case` etc.). Ele não mostra
nos tipos quais falhas uma função pode causar, o que combina mal com a tipagem estática. As falhas que podem
acontecer são escritas nos tipos com `Result`, e as transferências de controle são feitas com `catch` /
`throw`.

## 9. O que ler em seguida

- [Concorrência](concurrency.md): tarefas e canais
- [Option, Result e tipos de erro](../reference/functions/option-result.md): a lista de funções
- [Mensagens de erro](../reference/errors.md): o que significam os erros mais comuns e como corrigi-los

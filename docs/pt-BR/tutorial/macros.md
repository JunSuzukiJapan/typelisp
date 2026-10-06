<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Macros

Uma macro é uma função que recebe um programa e devolve um programa. As macros permitem criar uma sintaxe
nova que as funções não conseguem expressar. As macros do typelisp funcionam do mesmo jeito que o `defmacro`
do Common Lisp. Este capítulo pressupõe que você leu "Listas (expressões S)" em [Primeiros passos](intro.md).

## 1. Em que as macros diferem das funções

Uma função recebe seus argumentos **depois de avaliados**. Uma macro os recebe **como expressões, antes da
avaliação** (como dados de expressões S), constrói outra expressão e a devolve. A expressão devolvida
substitui a chamada da macro, e só então o tipo é verificado e ela é executada. Essa substituição se chama
**expansão**.

Por exemplo, uma sintaxe como `unless` não pode ser escrita como função. Como função, o corpo seria avaliado
primeiro mesmo quando a condição é verdadeira.

## 2. `defmacro` e quasiquote

Vamos fazer `my-unless`, que executa o corpo só quando a condição é falsa.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Os argumentos de uma macro não têm tipos escritos. Todo argumento é um dado de expressão S.
- `&rest body` recebe os argumentos restantes juntos como uma lista.
- Uma expressão que começa com `` ` `` (quasiquote) é construída como dado, tal como está escrita. Dentro
  dela:
  - `,test` insere o conteúdo da variável `test` nessa posição.
  - `,@body` encaixa os elementos da lista `body` nessa posição.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Você pode conferir a expansão com `macroexpand-1`. Ao escrever uma macro, olhar primeiro a expansão é o
caminho mais rápido.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. As expansões também passam pela verificação de tipos

A expressão que uma macro devolve tem o tipo verificado como qualquer expressão escrita à mão.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

O erro é relatado no lugar onde a macro foi chamada.

As regras de que o ramo else do `if` não pode ser omitido e de que os dois ramos de um `if` devem ter o mesmo
tipo valem tal e qual para as expansões. O `my-unless` acima termina com `(progn ,@body ())` para que,
qualquer que seja o tipo da última expressão do corpo, os dois ramos do `if` tenham o tipo `()`.

## 4. Conflitos de nomes e `gensym`

Uma macro direta que troca os valores de duas variáveis fica assim:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Funciona na maior parte do tempo, mas quebra quando a variável de quem chama se chama justamente `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (não trocou)
```

A expansão é `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, e o `tmp` que a macro criou esconde o
`tmp` de quem chama.

Para evitar isso, crie os nomes das variáveis usadas dentro de uma macro com `gensym`. `gensym` devolve um
símbolo novo que não pode ser escrito em lugar nenhum de um programa.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Como no Common Lisp, as macros do typelisp não evitam automaticamente conflitos de nomes (não são
higiênicas). Lembre-se: **use `gensym` para os vínculos que uma macro cria.**

Da mesma forma, uma macro que repete o corpo um dado número de vezes pode ser escrita assim:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Expandir de forma diferente conforme os argumentos

O corpo de uma macro é código typelisp comum, então pode examinar seus argumentos com `if` ou `match` e
construir uma expansão diferente. Os argumentos são dados de expressões S (`Option<Sexpr>`), e a lista vazia
é `none`.

Vamos fazer `my-and`, que devolve `true` se todas as suas condições forem verdadeiras.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; sem argumentos
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; só um
         `(if ,f (my-and ,@more) false)))             ; dois ou mais
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` é um padrão que pega a cabeça de uma lista em `f` e o resto em `more`.
- `sexpr-null` testa se um dado de expressão S é a lista vazia.
- O ramo final `_` é necessário porque os dados de expressões S têm formas além das listas (números, strings
  etc.), e `match` exige que elas também sejam cobertas. Um argumento `&rest` é sempre uma lista, então esse
  ramo nunca é executado de fato.
- Uma macro pode chamar a si mesma na sua expansão. A expansão se repete até não restarem chamadas de macro.

## 6. Argumentos opcionais

`&optional` recebe argumentos que podem ser omitidos. É possível dar valores padrão.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` recebe argumentos de palavra-chave
([Referência de sintaxe 3.14](../reference/syntax.md#314-defmacro--definição-de-macros)).

## 7. `macrolet`: macros para um único lugar

Uma macro usada só dentro de uma expressão pode ser definida com `macrolet`. Ela não é visível fora.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Coisas a ter em mente

- **Uma macro só pode ser chamada depois da sua definição.** Como com as funções, defina-a perto do início do
  arquivo.
- Torne uma macro disponível para outros módulos com `(pub defmacro ...)`.
- Boa parte da sintaxe padrão, incluindo `when`, `unless`, `cond`, `and`, `or` e `dotimes`, é definida como
  macros. Você pode ver o conteúdo com `(macroexpand '(when true 1))`.
- Se algo pode ser escrito como função, escreva como função. Macros não podem ser passadas como valores, e é
  preciso ler a expansão para entender o que fazem.

## 9. O que ler em seguida

- [Tratamento de erros](errors.md): `Result`, `panic`, `catch` / `throw`
- [Funções relacionadas a macros](../reference/functions/system.md#8-macros): `gensym`, `macroexpand` e mais

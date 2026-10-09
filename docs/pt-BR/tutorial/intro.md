<!-- translated-from: docs/ja/tutorial/intro.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Primeiros passos

Começando pela avaliação de expressões no REPL, este capítulo aborda, nesta ordem, funções, variáveis,
condicionais, laços, e listas e `Vector`. Para compilar o `typl`, consulte o
[README.md](../../../README.md).

## 1. Iniciar o REPL

Iniciado sem argumentos, o `typl` entra no REPL (modo interativo). Digite uma expressão depois de
`typl>` e ela é avaliada na hora e seu valor é impresso. `:quit` sai do REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Daqui em diante, a entrada e os resultados do REPL são mostrados desta forma.

## 2. Avaliar expressões

typelisp é um Lisp, então uma expressão fica entre parênteses com **o operador ou o nome da função
primeiro**. Escreve-se `(+ 1 2)`, não `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Os números são destes tipos:

- Os **inteiros** têm o tipo `int`. Não há limite superior para seu tamanho.
- Os **decimais** têm o tipo `f64`. Escreva-os com ponto decimal, como `1.5` ou `2.0`.
- Não é possível misturar `int` e `f64` em um cálculo. `(+ 1 2.0)` é um erro de tipo. Para converter,
  escreva `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` entre dois inteiros dá um inteiro com a parte fracionária descartada (não produz uma fração como no
Common Lisp). Para o resto, use `(mod 7 2)`.

Os valores booleanos são `true` e `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Definir funções

Funções são definidas com `defun`. **Os tipos dos argumentos e o tipo de retorno são sempre escritos.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` significa "um argumento `n` do tipo `int`". Com vários argumentos, liste-os:
  `((a int) (b int))`.
- O `int` depois da lista de argumentos é o tipo de retorno.
- O valor da última expressão do corpo é o valor de retorno da função. Não se escreve `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Uma chamada cujos tipos não batem é relatada como erro de tipo **antes de ser executada**. Ao executar um
arquivo, basta um único erro de tipo em qualquer lugar para que nenhuma linha do programa seja executada.

Para tornar um argumento opcional, use `&optional`. Se você der um valor padrão, o argumento assume esse
valor quando é omitido.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

O `false` passado como primeiro argumento de `format` significa "devolver o resultado como string em vez de
imprimi-lo". Cada `~a` é substituído pelo próximo argumento.

## 4. Variáveis

Variáveis locais são criadas com `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- O tipo de uma variável de `let` vem do seu valor inicial. Não é preciso escrevê-lo.
- As variáveis de um mesmo `let` não podem se referir umas às outras. Para construir uma variável a partir
  da anterior, use `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Para mudar o valor de uma variável, use `setf`. **A atribuição não pode mudar o tipo da variável.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Variáveis globais são definidas com `defvar`. Aqui você escreve o tipo.

```lisp
(defvar (counter int) 0)
```

## 5. Condicionais

### if

Escreva `(if condição expressão-se-verdadeiro expressão-se-falso)`. **A expressão do caso falso não pode ser
omitida.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Somente uma expressão do tipo `bool` pode ser condição. Escrever um número, como em `(if 0 ...)`, é um erro
  de tipo.
- As expressões dos casos verdadeiro e falso devem ter o mesmo tipo.

Quando nada deve acontecer no caso falso, use `when` (e `unless` para o contrário).

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

Com três ou mais condições, `cond` fica mais legível. O `else` final é tomado quando nenhuma das condições
vale.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Para ramificar conforme a forma de um valor, use `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` casa com qualquer valor. Como `int` tem incontáveis valores, deixar de fora o ramo `_` é um erro que diz
que nem todos os casos são cobertos. Onde `match` realmente brilha é ao desmontar `Option` e os tipos que
você mesmo define, que aparecem no próximo capítulo, [Fundamentos de tipos](types.md).

## 6. Laços

Uma função pode chamar a si mesma.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Para um número fixo de repetições, use `dotimes`. `i` vai de 0 a `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Também existem `while`, `do` e o `loop` estendido do Common Lisp. As palavras de cláusula do `loop`
estendido são escritas como palavras-chave (`:for`, `:collect` etc.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#(1 4 9 16 25)
```

## 7. Listas e Vector

### Vector

Para guardar uma sequência de valores do mesmo tipo, use `Vector<T>`. `T` é o tipo dos elementos.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; imprime #(3 1 2)
```

- `(Vector::new)` sozinho não determina o tipo dos elementos, então dê o tipo com `(the Vector<int> ...)`.
- `(push v x)` acrescenta no final, `(get v i)` lê o elemento `i` e `(len v)` dá o comprimento.
- Um `get` com índice fora do intervalo interrompe o programa com um erro.

### lambda e funções de ordem superior

Funções anônimas são criadas com `lambda`. Como no `defun`, escrevem-se os tipos dos argumentos e do
retorno.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` e afins recebem um `Vector` transformado em **iterador** com
`(iter v)`. A coleção vem primeiro e a função depois. O `v` acima foi ligado com `let`, então não
pode ser usado fora desse `let`. O próximo exemplo primeiro define `v` com `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #(30 10 20)
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #(3 2)
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #(1 2 3)
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Para processar os elementos um a um, use `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Uma função que recebe uma função como argumento escreve o tipo desse argumento como
`(fn (tipos-dos-argumentos...) tipo-de-retorno)`. Uma função definida com `defun` pode ser passada como
valor pelo seu nome.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listas (expressões S)

As listas criadas com `'(1 2 3)` ou `(list 1 2 3)` são **dados de expressões S**. Seus elementos não precisam
ter o mesmo tipo.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Os dados de expressões S servem principalmente para lidar com os próprios programas, em macros
([Macros](macros.md)) e com `read`. Para dados cujos elementos têm um tipo conhecido, use `Vector<T>`. Uma
lista de expressões S pode ser percorrida com `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Um par de dois valores é criado com `cons` e desmontado com `car` e `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Escrever um programa em um arquivo

Um programa pode ser escrito em um arquivo (com a extensão `.typl`) e executado com `typl nome-do-arquivo`.
Use `println` para mostrar resultados.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` imprime com as mesmas diretivas que `format` e termina com uma quebra de linha. `print` não
  acrescenta a quebra.
- `~a` insere um valor em forma legível por pessoas, e `~s` em uma forma que pode ser lida de volta (strings
  ganham suas `"`).
- Um arquivo é lido de cima para baixo. **Uma função não pode ser chamada antes da sua definição.**

## 9. O que ler em seguida

- [Fundamentos de tipos](types.md): `Option`, `Result`, estruturas, enumerações, genéricos
- [Para programadores de Common Lisp](../guide/from-common-lisp.md): uma lista de diferenças para quem
  conhece Common Lisp

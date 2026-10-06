<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits

Um trait é uma promessa de que "este tipo suporta estas operações". Os traits permitem que vários tipos
compartilhem operações com o mesmo nome, de modo que uma função que as usa não precise ser escrita uma vez
para cada tipo. Funcionam quase exatamente como os traits do Rust. Este capítulo pressupõe que você leu
[Fundamentos de tipos](types.md).

## 1. Definir e implementar um trait

Vamos definir como o trait `Shape` as operações que devolvem a área e o nome de uma figura.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- O `()` depois do nome do trait é a lista de traits dos quais ele herda (seção 4). Deixe vazia se não houver
  nenhum.
- Cada linha declara um método. `Self` representa "o tipo que implementa este trait".

Para implementar um trait para um tipo, escreva um `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Os métodos implementados são chamados como funções comuns.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Deixar de fora mesmo um só dos métodos que o trait declara é um erro de tipo no `impl`.

## 2. Restrições de trait: "qualquer tipo que implemente este trait"

É possível pôr uma condição no parâmetro de tipo de uma função genérica com `where`. Isso se chama
**restrição de trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Graças a `(where (Shape T))`, o corpo pode usar `name` e `area` em valores de `T`. Sem a restrição nada se
saberia sobre `T`, então eles não poderiam ser chamados.

Passar um tipo que não implementa `Shape` é um erro de tipo na chamada.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Uma função genérica ganha uma cópia própria para cada tipo com que é chamada. Não há testes de tipo nem
desvios em tempo de execução.

## 3. Implementações padrão

Se um método de trait tem corpo, esse corpo é usado quando um `impl` omite o método.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe é a implementação padrão

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; a que é escrita aqui tem prioridade

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementar traits padrão

A biblioteca padrão também tem traits. Implementar um deles torna disponíveis para seu tipo as funções padrão
que o usam.

| Trait | Métodos a implementar | O que ele permite |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, o padrão `(= expr)` de `match` etc. |
| `Ord` | `less` | `less-equal`, `greater` etc. `Ord` herda de `Eq` |
| `print-object` | `print-object` | Como `println` e afins mostram os valores |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` etc. |
| `Error` | `message`, `source` | Usá-lo como tipo de erro ([Tratamento de erros](errors.md)) |

Vamos implementar `Eq` e `Ord` para um tipo que representa uma quantia de dinheiro. Como `Ord` herda de `Eq`,
o `impl` de `Eq` tem de vir primeiro.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (a implementação padrão de Ord)
```

Implementar `print-object` decide como `println` mostra o valor. O argumento `escape` é `true` quando se pede
uma forma que pode ser lida de volta, como com `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Combinado com uma restrição de trait, você pode escrever uma função que serve para qualquer tipo que
implemente `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Dado um `Vector` com valores `money` de 300, 900 e 100 nessa ordem, ela devolve `(some 900 yen)`.

## 5. `:dyn`: lidar juntos com valores de tipos diferentes

Todos os elementos de um `Vector<T>` têm o mesmo tipo, então valores `circle` e `rect` não podem ir em um
mesmo `Vector<circle>`. Para lidar juntos com "coisas que implementam `Shape`", use o tipo `:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Um valor `circle` ou `rect` colocado onde se espera um `:dyn Shape` é convertido automaticamente.
- Qual `area` a chamada `(area s)` executa é decidido em tempo de execução pelo tipo do que `s` contém.
- Colocar um valor cujo tipo não implementa `Shape` onde se espera um `:dyn Shape` é um erro de tipo.

Como escolher entre as restrições de trait da seção 2 e `:dyn`:

| | Restrição de trait (`where`) | `:dyn Trait` |
|---|---|---|
| Quando o método chamado é decidido | Antes da execução | Em tempo de execução |
| Misturar tipos em um mesmo `Vector` | Não é possível | É possível |
| Tipos utilizáveis | Sem restrição | Estruturas, enumerações, `int`, `string`, `f64` e outros (não `bool`, `char`, `symbol`, `i32` e similares) |

A lista exata de tipos que podem ser usados está em
[Referência de sintaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Alguns traits não podem ser usados com `:dyn`: aqueles cujos métodos usam `Self` para um argumento que não
seja `self` ou para o valor de retorno (como `equals` de `Eq`). Como o tipo só é conhecido em tempo de
execução, não há como produzir "um valor do mesmo tipo".

## 6. Restrições

- Mantenha a definição de um trait, seus `impl` e o código que o usa via `:dyn` em um único módulo
  (arquivo). Ainda não é possível tornar um trait visível para outros módulos.
- Tipos e traits compartilham um namespace. Dentro de um módulo, um tipo e um trait não podem ter o mesmo
  nome.

## 7. O que ler em seguida

- [Macros](macros.md): definir sintaxe própria
- [Referência de sintaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits): implementações gerais
  (blanket), tipos associados e mais
- [Traits padrão](../reference/functions/traits.md): a lista de traits da biblioteca padrão

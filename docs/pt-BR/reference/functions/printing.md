<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Impressão

`print`/`println`/`format`, as impressoras de um argumento, o pretty printer, `print-object` e as variáveis que
controlam a impressão. A lista de diretivas de formato está em [format.md](format.md). Ler e escrever streams
está em [Streams e arquivos](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` são todas **formas especiais que interpretam diretivas de formato (as diretivas do
`format` do CL)**. O primeiro argumento (o segundo em `format`) é a **string de controle**, e cada diretiva
consome por vez os argumentos variádicos que se seguem.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expande a string de controle e a escreve na saída padrão sem quebra de linha |
| `println` | `(println control args...)` | `(string, ...)→Unit` | O mesmo, com uma quebra de linha no final |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | O `format` do CL. Devolve a string expandida. Se `dest` for `true` (o `t` do CL), também é escrita na saída padrão; se for `false` (o `nil` do CL), não é escrita e só é devolvida |
| `format` (para um stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Se `dest` não for um `bool`, é o destino stream do CL. A string expandida é escrita nesse stream. O valor de retorno é `()` (o `nil` do CL), e nenhuma string é devolvida |

O tipo de `dest` divide o significado em dois (qual se aplica é decidido estaticamente). A forma com stream
pode ser escrita do mesmo jeito com um tipo de stream concreto, um `:dyn CharOutput` ou uma variável de tipo
vinculada por `(where (CharOutput S))`. Um `dest` que não é nem `bool` nem stream é um erro de tipo.

**A string de controle deve ser um literal** (a mesma restrição do `format!` do Rust). As diretivas que ela
contém decidem quantos argumentos são tomados e de que tipos, então uma string construída em tempo de execução
não pode ser lida na verificação. Por ser obrigatoriamente um literal, **o número e os tipos dos argumentos são
verificados na verificação**: `(println "~d" "x")` e `(println "~a ~a" 1)` são erros na verificação. Uma diretiva
digitada errado, um `~(` não fechado e um `~/name/` que nenhum argumento pode responder também são erros na
verificação. As regras de verificação estão em [format.md](format.md#1-como-escrever-diretivas). Para imprimir
uma string que você constrói, crie-a com `(format false ...)` e imprima-a com `(println "~a" s)`.

Os argumentos variádicos são envolvidos em `Sexpr` com seus próprios tipos antes de serem passados:
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, assim como `defstruct`/`defenum`/`Vector<T>`/
`HashTable<K,V>` definidos pelo usuário e afins, podem todos ser passados como estão (`(println "~a" my-struct)`
simplesmente funciona).

Executar um script com `typl file.typl` **não imprime os valores das expressões de nível superior**, então um
programa escreve na saída padrão chamando estas. `print`/`println`/`format` enviam sua saída a cada chamada
(para que um prompt fique visível antes de a entrada padrão ser lida, mesmo através de um pipe).

**`Option<Sexpr>` é impresso de forma transparente.** O tipo dos dados de expressões S é `Option<Sexpr>`, então
o invólucro `(some x)` não aparece na saída e o conteúdo é impresso como está. A lista vazia é impressa como
`()`. Os outros `Option<T>` são impressos como `(some ...)` / `none`. O mesmo vale para campos `Option<T>`
dentro de estruturas, enumerações e `Vector`. Um `Result<Option<Sexpr>,…>` de `(eval ...)` é impresso como
`(ok 42)`, ou `(ok ())` para `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; obter só a string, sem imprimir
  (println "~a" s))                   ; => id=42
```

## 2. Impressoras de um argumento

As impressoras do CLHS 22.1.3. Em vez de expandir um formato, imprimem um único valor como está. O stream pode
ser omitido (o padrão é `*standard-output*`).

| Nome | Forma | Descrição |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Escreve em uma forma que pode ser lida de volta (o mesmo que `~s`) e devolve `x` |
| `princ` | `(princ x [stream])` | Escreve em uma forma para pessoas (o mesmo que `~a`) e devolve `x` |
| `write` | `(write x [stream])` | `prin1` se `*print-escape*` for verdadeiro, `princ` se for falso. Devolve `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Devolve uma string em vez de escrever (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | O mesmo (`~a`). O mesmo que `to-string` |
| `write-to-string` | `(write-to-string x)` | O mesmo, seguindo `*print-escape*` |

`print`/`println` **não** estão entre elas. São abreviações de `format` que recebem uma string de controle, um
trabalho diferente do `print` do CL (quebra de linha, depois `prin1`, depois um espaço), então cada um mantém
seu próprio nome. Como resultado, **o `print` de um argumento do CL não tem escrita nesta linguagem**: escreva
`prin1`.

São macros, porque os argumentos variádicos de `format` não aceitam variáveis de tipo e o tipo precisa ser
conhecido no ponto de chamada.

## 3. A entrada padrão e os streams padrão

**Ler a entrada padrão** não é feito com funções dedicadas, mas com os métodos de `CharInput` sobre o stream
padrão `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([métodos de stream](streams-files.md#2-métodos)). A saída padrão e a saída de
erro padrão também têm `*standard-output*` / `*error-output*`, e podem ser escritas como em
`(write-line *standard-output* s)` (`print`/`println`/`format` são atalhos para quando você precisa expandir um
formato, e sempre escrevem na saída padrão).

## 4. O pretty printer

Corresponde ao Lisp Pretty Printer do CL (CLHS 22.2). **Divide a saída que não cabe na largura da linha,
seguindo os blocos lógicos e as quebras de linha condicionais.**

### 4.1 Variáveis de controle

Variáveis globais que podem ser atribuídas. Depois de um `setf`, afetam toda a impressão posterior. Para
mudar uma temporariamente, use `dlet` (6.3).

| Variável | Tipo | Padrão | Significado |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Se verdadeiro, `~a`/`~s`/`~w` e as diretivas pretty tomam o caminho da impressão bonita |
| `*print-right-margin*` | `int` | `80` | A margem direita (em colunas). 0 significa "sem margem, nunca dividir". Um valor negativo é um erro de impressão |
| `*print-miser-width*` | `int` | `0` | A largura a partir da qual começa o estilo miser. 0 corresponde ao `nil` do CL (estilo miser desligado). Um valor negativo é um erro de impressão |

A família `pprint` e `pprint-logical-block` sempre fazem impressão bonita, independentemente de
`*print-pretty*` (seguindo a definição do `pprint` do CL).

### 4.2 Layouts prontos (formas especiais)

Como `print`, são formas especiais, então o argumento pode ser de qualquer tipo.

| Nome | Forma | Descrição |
|---|---|---|
| `pprint` | `(pprint x)` | Imprime bonito com o layout padrão. Como no CL, **escreve primeiro uma quebra de linha** e nenhuma no final |
| `pprint-fill` | `(pprint-fill x)` | Preenche cada linha com o quanto couber. Não escreve quebra de linha |
| `pprint-linear` | `(pprint-linear x)` | Se nem todos os elementos couberem em uma linha, **um elemento por linha**. Não escreve quebra de linha |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Uma tabela com colunas de `colinc` de largura (16 por padrão). Não escreve quebra de linha. Um `colinc` negativo é um erro |

O layout padrão (`pprint`, e `~a` com `*print-pretty*`) segue o `*print-pprint-dispatch*` padrão do CL: abrevia
`(quote x)` como `'x`, e formata as formas de código como `defun`/`let`/`if`/`lambda` como "a cabeça e o número
prescrito de argumentos na primeira linha, e o resto do corpo recuado duas colunas, uma forma por linha". As
outras listas são preenchidas.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Construir blocos lógicos você mesmo

| Nome | Forma | Descrição |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Uma forma especial que abre um bloco lógico. `obj` é a lista que `pprint-pop` percorre (`()` se nenhuma for percorrida). `:prefix` e `:per-line-prefix` são mutuamente exclusivos (como no CL) |
| `pprint-newline` | `(pprint-newline kind)` | Uma quebra de linha condicional. `kind` é `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Recuo. `kind` é `:block` (a partir do início do bloco) / `:current` (a partir da coluna atual) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Uma tabulação. `kind` é `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` e `colinc` não são negativos (erro se forem) |
| `pprint-pop` | `(pprint-pop)` | Tira o próximo elemento da lista do bloco (`()` se tiver acabado) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Se a lista acabou |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Se acabou, faz `break` do `loop` envolvente (uma macro) |

Os blocos lógicos não recebem um argumento stream: **um bloco lógico aberto é um estado implícito**. O
`pprint-logical-block` mais externo o inicia, e quando ele fecha, tudo é formatado e escrito de uma vez na
saída padrão. Enquanto ele está aberto, a saída de `print`/`println`/`(format true ...)`/`pprint` vai toda para
esse bloco, então **você escreve o conteúdo com `print` comum e marca só os lugares onde dividir com
`pprint-newline` e afins**, o que faz o código ficar quase igual ao do CL.

No CL, `pprint-exit-if-list-exhausted` é uma saída não local de `pprint-logical-block`; aqui é **um `break` do
`loop` envolvente** (`pprint-logical-block` não estabelece um `block`). O idioma do CL sempre o coloca dentro
de um `loop` de qualquer forma, então se lê do mesmo jeito.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Regras das quebras de linha condicionais (CLHS `pprint-newline`):

- `:mandatory` sempre divide.
- `:linear` divide se o bloco lógico envolvente não couber em uma linha. A decisão é por bloco, então **todas
  as quebras `:linear` de um bloco dividem juntas** (é o "tudo em uma linha ou um elemento por linha" de
  `pprint-linear`).
- `:fill` divide se (a) a próxima seção não couber no resto da linha, (b) a seção anterior não coube em uma
  linha, ou (c) no estilo miser, o bloco não couber em uma linha.
- `:miser` funciona como `:linear` só no estilo miser (quando o bloco começa a menos de
  `*print-miser-width*` da margem direita).

## 5. `print-object` (representação impressa por tipo)

Escrever `impl print-object <tipo>` faz com que `print`/`println`/`format`/`pprint` imprimam os valores desse
tipo com essa implementação, **mesmo quando estão aninhados dentro de listas**. Corresponde à função genérica
`print-object` do CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argumento | Significado |
|---|---|
| `self` | O valor a imprimir |
| `escape` | O `*print-escape*` do CL. `true` para `~s`/`prin1`/`pprint` (uma forma que pode ser lida de volta), `false` para `~a`/`princ` (para pessoas). Uma implementação que não se importe pode ignorá-lo |

A `string` devolvida vai direto para a saída. Os tipos sem `impl` são impressos na representação embutida (da
forma `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   também funciona aninhado
```

Também se combina com o pretty printer (capítulo 4). Se `*print-pretty*` for verdadeiro, uma lista contendo as
strings que a implementação devolveu é dividida na margem direita.

As representações impressas dos tipos da biblioteca padrão. Os tipos que também existem no CL são impressos do
mesmo jeito que no SBCL. Quando o REPL mostra um resultado, usa a mesma representação que `~s`.

| Tipo | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Igual |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (o número é um número de série interno) | Igual |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Um inteiro (o valor de `get-universal-time` / `get-internal-real-time` do CL) | Igual |
| Tipos de erro (`ParseIntError`, `SimpleError` etc.) | `#<simpleerror "boom">` | Só a mensagem (`boom`) |
| `complex` | `#C(1.0 2.0)` | Igual |
| `Array<T>` | `#2A((0 0) (0 0))` | Igual |
| Streams | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Igual |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Igual |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` no final durante o horário de verão) | Igual |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Igual |
| Tipos `defstruct` | `#<point x: 1 y: 2>` (nomes e valores dos campos) | Igual (campos com `~a`) |

Regras:

- **O registro é estático.** Um `impl` tem o tipo verificado como uma definição de método comum, então um nome
  de tipo digitado errado ou uma assinatura errada é um erro de compilação.
- **Também funciona para tipos genéricos.** `(impl print-object box<T> (where (print-object T)) ...)` vai para
  um corpo separado para cada argumento de tipo: um valor lembra seu tipo incluindo seus argumentos de tipo
  (`box<i32>`). Os tipos genéricos embutidos como `Vector<T>` funcionam do mesmo jeito.
- **A escolha é feita na hora de imprimir.** Qual diretiva consome qual argumento depende do conteúdo em tempo
  de execução da string de controle, então a distinção entre `~a` e `~s` (isto é, `escape`) só é conhecida no
  momento da impressão. É o mesmo que no CLOS, em que os métodos `print-object` "são definidos por classe e
  escolhidos na hora de imprimir".
- **A reentrada recai na representação embutida.** Se uma implementação imprimir a si mesma com
  `(format false "~a" self)`, entraria em recursão para sempre, então quando um valor que está sendo impresso
  aparece de novo, usa-se a representação embutida. Isso olha a identidade do valor, não um limite de
  profundidade, então não atrapalha a impressão legítima de estruturas autorreferentes aninhadas.
- **Todo tipo escalar implementa este trait.** Isso é **para que ele possa ser usado como restrição**: os
  argumentos variádicos de `format` não podem receber variáveis de tipo, então essa restrição é a única forma
  de o código genérico dizer "valores de um tipo desconhecido podem ser representados" (a mesma forma do
  `T: Display` do Rust). O `print-object` de `Array<T>` é um exemplo.
- **Com argumentos de tipo que não atendem à restrição, a representação embutida é usada silenciosamente.**
  `(impl print-object Array<T> (where (print-object T)))` se aplica a `Array<i32>`, mas não a um `Array` cujos
  elementos são um `defstruct` sem `print-object`. Não faria sentido que simplesmente criar um array fosse um
  erro, então não é um erro.
- O outro mecanismo do CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (um registro em tempo de execução
  indexado por especificadores de tipo), **não é adotado**. Seus registros não são verificados, o que não
  combina com uma linguagem de tipagem estática.

## 6. Controlar quanto é impresso

### 6.1 Profundidade, comprimento e compartilhamento

As variáveis de controle do CLHS 22.1.1 que decidem "quanto de um valor é impresso". Como as três de 4.1, são
globais que podem ser atribuídas, e se aplicam a todo `print`/`println`/`format`/`pprint`, seja
`*print-pretty*` verdadeiro ou não.

| Variável | Tipo | Padrão | Significado |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Os objetos aninhados nesta profundidade ou mais são substituídos por `#`. O objeto sendo impresso está na profundidade 0. 0 significa ilimitado |
| `*print-length*` | `int` | `0` | Imprime os elementos das listas (e os campos dos valores `defstruct`/`defenum`) até esta quantidade e substitui o resto por `...`. 0 significa ilimitado |
| `*print-circle*` | `bool` | `false` | Se verdadeiro, o valor é examinado antes da impressão e **os objetos que aparecem duas ou mais vezes recebem rótulos**. A primeira ocorrência é `#n=…` e as seguintes `#n#` |

O CL usa `nil` para "ilimitado", mas esta linguagem não tem `nil`, então, como com `*print-right-margin*`, **0
significa ilimitado**. Valores negativos não têm sentido e são erros de impressão. Os padrões são todos "sem
limite / sem rótulos", iguais aos valores iniciais do CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**As estruturas circulares só podem ser impressas quando `*print-circle*` é verdadeiro.** Se você imprimir um
valor que aponta para si mesmo enquanto ele é falso (o padrão), a impressora segue o ciclo indefinidamente e o
processo cai. O CL é igual (o CLHS deixa indefinida a impressão de estruturas circulares quando
`*print-circle*` é falso).

Um ciclo só pode ser criado "apontando com `setf` um campo de `defstruct` para si mesmo" (as células `Sexpr` não
podem ser mudadas depois de criadas, então uma lista como `'(1 2 3)` nunca pode ser circular):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a aponta para o próprio a
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Os rótulos **recomeçam de 1 a cada coisa impressa** (como no CL). Mesmo sem ciclo, se o mesmo objeto aparecer
duas vezes ele recebe `#1=`/`#1#`, mantendo na saída a informação de que "estes dois são o mesmo objeto", como o
CL especifica:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Um valor sem compartilhamento **não mostra rótulo nenhum**, então deixar esta variável como verdadeira não muda a
saída do código do dia a dia.

### 6.2 Base, caixa e legibilidade

| Variável | Tipo | Padrão | Significado |
|---|---|---|---|
| `*print-base*` | `int` | `10` | A base para imprimir inteiros (de largura fixa e `int`). Fora de 2 a 36 é um **erro de impressão** (o CL também especifica a faixa) |
| `*print-radix*` | `bool` | `false` | Se verdadeiro, acrescenta uma marca de base: `#b`/`#o`/`#x`, `#NNr` para outras bases, e um `.` final para a base 10. A marca vem **antes** do sinal (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | A caixa dos nomes de símbolo: `:upcase` / `:downcase` / `:capitalize` (as mesmas grafias do CL). Qualquer outro símbolo é um erro de impressão |
| `*print-readably*` | `bool` | `false` | Se verdadeiro, imprime em uma forma que pode ser lida de volta. Força o escape e desativa os cortes de `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | O número de linhas que o pretty printer pode usar. O excesso é cortado, com `..` no final como no CL. 0 significa ilimitado. Um valor negativo é um erro de impressão |
| `*print-escape*` | `bool` | `true` | Se `write`/`write-to-string` fazem `prin1` ou `princ`. **Só esses dois o leem** |
| `*print-array*` | `bool` | `true` | Se `Vector<T>` e `Array<T>` mostram o conteúdo. Se verdadeiro, a sintaxe de arrays de CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); se falso, só o tipo e a forma, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

As marcas que `*print-radix*` acrescenta podem ser lidas de volta pelo leitor (a notação de base da
[Referência de sintaxe](../syntax.md#1-elementos-léxicos)).

**Por que o padrão de `*print-case*` difere do CL**: o padrão do CL é `:upcase` porque o leitor do CL guarda os
nomes de símbolo em maiúsculas, isto é, significa "como foram guardados". Este leitor os guarda em minúsculas,
então o padrão com o mesmo significado é `:downcase`.

**A metade que falta de `*print-readably*`**: o CL sinaliza `print-not-readable` para valores que não podem ser
lidos de volta, mas esta linguagem não tem condição a sinalizar, nem como decidir a legibilidade dos tipos de
usuário, que `print-object` pode imprimir de qualquer jeito. Só estão presentes o escape forçado e a anulação
dos cortes.

**Por que só `write` lê `*print-escape*`**: como o CLHS especifica, `~s`/`prin1`/`pprint` o vinculam a
verdadeiro, e `~a`/`princ` a falso, cada um só durante a própria chamada. Então os únicos leitores que o veem
sem vínculo são `write`/`write-to-string`. Uma implementação de `print-object` deve ler seu próprio argumento
`escape` em vez desta global: esse argumento carrega o valor que a diretiva escolheu.

**O que o CL tem e esta linguagem não**: `*print-gensym*` (não há símbolos não internados).

### 6.3 Substituições temporárias

O CL os vincula com `let`, mas `let` nesta linguagem vincula lexicamente, então use `dlet`
([Outros](system.md#10-outros)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; os limites valem só para esta impressão
(with-standard-io-syntax (println "~a" x))   ; imprimir com tudo de volta aos valores padrão
```

`with-standard-io-syntax` executa seu corpo com todas as variáveis de controle da impressora em seus valores
padrão e `*read-eval*` como `true`.

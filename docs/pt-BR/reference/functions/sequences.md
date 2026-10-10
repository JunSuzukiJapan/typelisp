<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Pares, expressões S e sequências

O par genérico `cons-cell`, os dados de expressões S `Sexpr`, os símbolos, as funções de sequência escritas
sobre `Iter` e as funções de ordem superior.

## 1. Pares `cons-cell<A,B>`

`cons`/`car`/`cdr` são o construtor e os acessores de campo do **tipo par genérico `cons-cell<A,B>`** (um
`defstruct` da biblioteca padrão). Os campos podem ser lidos como `variável::car`/`variável::cdr` (a sintaxe de
acessores de `defstruct` da
[Referência de sintaxe](../syntax.md#36-defstruct--estruturas-tipos-definidos-pelo-usuário)) ou como
`(car variável)`/`(cdr variável)`. Para mudá-los, use `(setf variável::car v)`/`(setf variável::cdr v)`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Cria um par |
| `car` | `(car p)` | `cons-cell<A,B>→A` | O primeiro elemento |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | O resto |

`cons-cell` também serve no lugar de uma sintaxe de tuplas. As funções do CL que devolvem valores múltiplos (o
quociente e o resto de `floor`, o valor e a posição de `read-from-string` etc.) devolvem um `cons-cell` nesta
linguagem.

## 2. Dados de expressões S `Sexpr`

O tipo de dados `Sexpr` devolvido por `read` tem 19 variantes:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` e `array` são dados escritos como `#(..)` e `#nA(..)` ([referência de
sintaxe](../syntax.md#1-elementos-léxicos)), que contêm respectivamente um `Vector<Option<Sexpr>>` e
um `Array<Option<Sexpr>>`: `len`, `get` e o resto funcionam diretamente sobre o `v` que `(vector v)`
`tuple` são dados escritos com `#{..}`, e o `v` que `(tuple v)` vincula é um `Vector<Option<Sexpr>>`
novo com os elementos (para receber com um único tipo uma tupla de qualquer tamanho).
vincula.
As células de expressões S não são tratadas pelos `cons`/`car`/`cdr` gerais do capítulo 1, mas pelas funções
`sexpr-*`. Elas são usadas principalmente nos corpos de `defmacro` para construir e desmontar formas.

**O tipo dos dados de expressões S é `Option<Sexpr>`.** A lista vazia não é uma variante de `Sexpr`, mas o
`none` de `Option`, e `Sexpr` em si significa "uma expressão S não vazia". Então as funções `sexpr-*` recebem e
devolvem `Option<Sexpr>`.

- `()` é a lista vazia onde se espera um `Option<Sexpr>` (também pode ser escrita `(Option::none)`)
- `Sexpr` é ampliado implicitamente onde se espera um `Option<Sexpr>` (sem conversão em tempo de execução). A
  direção oposta, usar um `Option<Sexpr>` como `Sexpr`, afirma "isto não é a lista vazia", então precisa ser
  declarada explicitamente com `match` ou `unwrap`
- No `match`, as 19 variantes de `Sexpr` e `none` podem ser escritas **planas na mesma lista de ramos**
  ([Referência de sintaxe](../syntax.md#43-match--casamento-de-padrões))

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Cria uma célula `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | O primeiro elemento. **A lista vazia para a lista vazia** (como no CL). Panic com um átomo que não é `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | O resto. **A lista vazia para a lista vazia** (como no CL). Panic com um átomo que não é `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Se é um `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Se é a lista vazia |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Se não é um `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Se é um `Sym` (símbolo) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | O conteúdo da variante `int` (fixnum ou bignum). Panic com outro tipo |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | O conteúdo da variante daquela largura. Panic com outro tipo |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | O conteúdo das variantes de ponto flutuante. Panic com outro tipo |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | O conteúdo de um `Char`. Panic com outro tipo |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | O conteúdo de um `Bool`. Panic com outro tipo |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | O conteúdo de um `Str`. Panic com outro tipo |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | O nome de um `Sym`. Panic com outro tipo |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Comparação de identidade (`Cons`/`Str` comparam a identidade do objeto, o resto compara valores) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Igualdade estrutural (`Cons` recursivamente, `Str` pelo conteúdo) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Como `equal`, mais comparação sem diferenciar maiúsculas e comparação de números entre tipos |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Concatena duas listas `Sexpr` (sem destruí-las). `,@` se expande para isto |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Uma lista `Sexpr` nova com `f` aplicada a cada elemento de uma lista `Sexpr` (o `map` do capítulo 4 é para `Iter` e não consegue percorrer uma lista `Sexpr`) |

Há nove acessores numéricos, um por tipo, porque um `Sexpr` é "o único lugar onde o tipo de um valor não está
escrito em nenhum outro lugar". Um `u8` colocado em um `Sexpr` entra como a variante `u8` e só sai com
`(sexpr-u8 s)`. Passá-lo a `(sexpr-int s)` causa panic; nunca amplia a resposta silenciosamente. Os inteiros
dos dados lidos (`'(1 2 3)`, argumentos de macro) são da variante `int` e são lidos com `(sexpr-int s)`.

As listas `Sexpr` não têm operações destrutivas como `rplaca`/`nconc`. Uma célula `Sexpr` não pode ser mudada
depois de criada.

## 3. Símbolos

`symbol` é o tipo dos próprios símbolos. Ele é convertido implicitamente onde se exige um `Sexpr`, mas não
automaticamente na outra direção.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Tira o nome do símbolo |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Cria um símbolo a partir de uma string (internando-o) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Se é uma palavra-chave (`:name`). Os dois-pontos fazem parte do nome, então o teste olha o primeiro caractere ([Referência de sintaxe](../syntax.md#1-elementos-léxicos)) |

Para `gensym`, consulte [Macros](system.md#8-macros).

## 4. Funções de sequência sobre `Iter`

As funções de sequência são **funções genéricas sobre o trait `Iter`**. De uma coleção, obtenha um iterador com
`(iter coll)` e passe-o (`Vector<T>` / `HashTable<K,V>` / `Array<T>` suportam isso; uma lista `Sexpr` não
implementa `Iter`, então estas funções não se aplicam a ela). **Uma coleção resultante é devolvida como um
`Vector` novo.** `Iter<A>` nas tabelas significa "qualquer implementação de `Iter` cujo `Item` seja `A`". Para
percorrer de novo o `Vector` devolvido, passe `(iter result)`.

Funções que recebem um predicado (correspondem à família `-if` do CL):

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Mapeamento |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Só os elementos que satisfazem o predicado |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Remove os elementos que satisfazem o predicado |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | O primeiro elemento que satisfaz o predicado |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | A primeira posição que satisfaz o predicado |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Quantos satisfazem o predicado |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Se todo elemento satisfaz o predicado |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Se algum elemento satisfaz o predicado (corresponde ao `some` do CL; um nome que não colide com o construtor `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Dobra à esquerda |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Dobra à direita |

Índices, comprimento e fatiamento:

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Número de elementos |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Concatena iteradores. Podem ser dados três ou mais |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | O `concatenate` do CL. O tipo do resultado é escrito como **um literal de símbolo citado** (o CL usa um especificador de tipo em tempo de execução). `'vector` recebe um ou mais, `'string` zero ou mais (`""` para zero). As listas `Sexpr` não estão incluídas (use `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Inversão (não destrutiva) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | O elemento `n` (`None` fora do intervalo) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` com os argumentos ao contrário |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Os primeiros `n` elementos |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` é limitado ao comprimento) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | O último **elemento** (não "a última célula" como no CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Todos menos o último elemento |

Funções que exigem uma restrição `Eq` / `Ord` (comparam por meio de um trait em vez de um predicado;
[Traits padrão](traits.md#2-eq--ord-comparação)):

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Se existe um elemento igual a `x` (ao contrário do CL, um `bool`, não o resto da lista) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | O primeiro elemento igual a `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | A primeira posição igual a `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Quantos elementos são iguais a `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | O `(sort sequence predicate)` do CL. Uma ordenação estável e não destrutiva. `cmp` é `true` quando "o primeiro argumento vem estritamente antes do segundo" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | O primeiro par cujo `car` é igual a `k`. Tire o valor com `(cdr p)` |

Estas e muitas das funções do capítulo 5 também recebem os argumentos de palavra-chave do CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (capítulo 6).

## 5. O resto das funções de sequência do CL

Todas são funções genéricas sobre `Iter`, como no capítulo 4. As coleções resultantes são devolvidas como
`Vector` novos.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Os índices nomeados do CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Todos menos o primeiro (um `Vector` novo, não uma cauda compartilhada) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materializa um iterador em um `Vector` (o `copy-seq`/`copy-list` do CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` invertido, seguido de `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` cópias de `x` (o `make-list`/`make-sequence` do CL). Como com `Vector::new`, o argumento de tipo vem do tipo esperado, então um `let` simples precisa de `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Como `member`, um **`bool`** (um iterador não tem cauda para devolver) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | As negações de `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Os mesmos tipos das versões positivas | Versões com o predicado negado |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Remove por valor |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Remove duplicatas. Como no CL, **a última ocorrência é mantida** (`:from-end true` mantém a primeira) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Substitui por valor / por predicado |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | sobre `Iter<cons-cell<K,V>>` | As versões com predicado e do lado do valor de `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Acrescenta um par na frente |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Emparelha duas sequências. Para na mais curta |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | O `mapcar` do CL sobre várias sequências. Para na mais curta |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Mapeamento pelos efeitos colaterais |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Mapeia e concatena |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Mapeia sobre as **caudas** sucessivas |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Mapeia sobre as caudas pelos efeitos colaterais (a contrapartida em `maplist` de `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Mapeia sobre as caudas e concatena (a contrapartida em `maplist` de `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | A posição em que `sub` aparece pela primeira vez. Se o receptor for uma `string`, o método de `string` é escolhido ([Strings](collections.md#1-strings-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | A primeira posição em que diferem. `none` se forem iguais |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Mesclagem. O CL exige entradas ordenadas; esta ordena a concatenação |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Acrescenta `x` **na frente** se ele não estiver lá |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Operações de conjunto. O CL não especifica a ordem; aqui ela é estável, **na ordem de primeira aparição** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inclusão |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Se é um sufixo / a parte antes do sufixo. O CL pergunta sobre **estrutura compartilhada**, mas não há estrutura a compartilhar, então aqui se pergunta sobre um sufixo **como valores** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Igualdade elemento a elemento. `Vector<T>` em si não implementa `Eq` |
| `caar`…`cddddr` | `(cadr p)` | sobre pares aninhados | As 28 funções do CL. Percorrem **pares, não listas**: `cadr` recebe um `cons-cell<A,cons-cell<B,C>>` |

O que o CL tem e esta linguagem não: `list*` (não existe a noção de uma lista imprópria cuja cauda é
substituída), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (nenhum tipo pode descrever o percurso de uma
árvore heterogênea de profundidade arbitrária; para uma árvore de `Sexpr`, `equal` corresponde a `tree-equal`),
a família das listas de propriedades `getf`/`get-properties`/`symbol-plist`/`remprop` (não há representação
como uma lista sem tipo que alterna chaves e valores; `assoc` (listas de associação) ou `HashTable` cumprem o
mesmo papel) e as funções que convertem entre `Vector<T>` e listas `Sexpr` (cada elemento de uma lista `Sexpr`
pode ter um tipo diferente, então não podem ser escritas com um único tipo de elemento `T`).

## 6. Argumentos de palavra-chave

As funções dos capítulos 4 e 5 recebem as palavras-chave de sequência do CL `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Todas são **opcionais**.

| Palavra-chave | Tipo | Significado |
|---|---|---|
| `:key` | `(fn (A) A)` | Uma projeção aplicada a cada elemento antes de comparar ou testar |
| `:test` | `(fn (A A) bool)` | Um teste de igualdade usado em vez de `equals` da restrição `Eq`. O primeiro argumento é **o item procurado**, o segundo é o elemento (depois de `:key`), na mesma ordem do CL |
| `:test-not` | `(fn (A A) bool)` | A negação de `:test` |
| `:start` `:end` | `int` | A janela `[start, end)` a percorrer. Os índices são relativos à sequência inteira |
| `:from-end` | `bool` | Uma busca responde com a **última** ocorrência. Combinado com `:count`, os elementos afetados são tomados a partir do fim |
| `:count` | `int` | O número máximo de elementos afetados pelas famílias `remove` / `substitute` |

Qual função recebe qual segue o CL:

| Função | Palavras-chave recebidas |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Todas as acima (incluindo `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (o `:key` de `assoc` se aplica ao `car`, o de `rassoc` ao `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; remove só um, a partir do fim
(position 3 (iter v) :start 1)                          ; o índice é relativo à sequência inteira
```

**Diferenças em relação ao CL**:

1. **A projeção de `:key` fica dentro do tipo dos elementos** (`(fn (A) A)`). Não pode projetar para outro tipo
   como no CL: uma variável de tipo extra não poderia ser determinada quando o argumento é omitido. Onde for
   preciso projetar para um tipo diferente, passe uma lambda para a família `-if`
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Nas buscas por item, `:key` se aplica só aos elementos** (não ao item procurado). É a mesma regra de
   `find`/`position`/`count`/`member`/`remove`/`substitute` do CL. Nas operações de conjunto ambos os lados são
   elementos, então se aplica aos dois.
3. **Só as palavras-chave de `search` têm nome em vez de número.** No CL, `:start1`/`:end1` são para o
   **padrão** e `:start2`/`:end2` para a sequência pesquisada. Nesta linguagem o receptor vem primeiro, então
   os mesmos números significariam o contrário, e silenciosamente. `:start`/`:end` são para o receptor e
   `:sub-start`/`:sub-end` para o padrão, de modo que um `:start1` distraído dá um erro de "palavra-chave
   desconhecida". `mismatch` e `replace` têm a mesma ordem de argumentos do CL, então mantêm os números do CL.

## 7. Operações destrutivas

Métodos de `Vector<T>`. **Modificam o receptor e devolvem o próprio receptor**, então `(nreverse v)` é escrito
do mesmo jeito que `reverse` e o próprio `v` também fica invertido.

| Nome | Forma | Descrição |
|---|---|---|
| `nreverse` | `(nreverse v)` | Inverte no lugar |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versões no lugar de `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versões no lugar da família `substitute` |
| `nbutlast` | `(nbutlast v)` | Descarta o último elemento |
| `fill` | `(fill v x)` | Põe todos os elementos como `x`. O comprimento não muda |
| `replace` | `(replace v src)` | Sobrescreve a partir do início com os elementos de `src`. `(min (len v) (len src))` elementos |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. A mesma contagem que acima |
| `nconc` | `(nconc v w)` | Acrescenta os elementos de `w` a `v`. Ao contrário do CL, **não reescreve estrutura compartilhada** (`w` não é afetado) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Substitui o conteúdo de `v` por `src` (o comprimento também muda) |
| `rplaca` `rplacd` | `(rplaca p x)` | Reescreve o `car`/`cdr` de um `cons-cell` e devolve a própria célula |

Palavras-chave recebidas:

| Versão destrutiva | Palavras-chave recebidas |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (o receptor é a `sequence-1` do CL) |

`vector-push-extend`/`vector-pop` são simplesmente `push`/`pop` de `Vector<T>`. Um `Vector<T>` sempre cresce,
então nada corresponde à distinção do CL entre "um vetor com ponteiro de preenchimento" e "um vetor simples".

## 8. Funções de ordem superior

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Devolve seu argumento |
| `const` | `(const x y)` | `(A,B)→A` | Devolve o primeiro argumento |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Composição de funções `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Troca os argumentos de uma função de dois argumentos |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negação de um predicado |

Não existe o `constantly` do CL (o tipo do argumento ignorado só apareceria no tipo de retorno e não poderia ser
determinado). Escreva `(lambda ((x T)) A v)`.

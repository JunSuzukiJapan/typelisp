<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Strings, caracteres e coleções

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` e `BitVector`.

## 1. Strings `string`

As strings são imutáveis.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Converte para maiúsculas (só ASCII). Como o `string-upcase` do CL, devolve uma string nova. As strings são imutáveis, então não existe o destrutivo `nstring-upcase`; esta ocupa o lugar dele |
| `downcase` | `(downcase s)` | `string→string` | Converte para minúsculas (só ASCII). Ocupa o lugar de `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Põe em maiúscula a primeira letra de cada palavra e o resto em minúsculas (o `string-capitalize` do CL). Uma palavra é uma sequência máxima de letras e dígitos |
| `length` | `(length s)` | `string→int` | Número de caracteres |
| `ref` | `(ref s i)` | `(string,int)→char` | O caractere `i`. Panic fora do intervalo |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | A substring `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Concatenação. Podem ser dadas três ou mais (o mesmo que `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Comparação lexicográfica |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Menor estrito lexicográfico (o mesmo que `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Comparação de identidade (se são o mesmo objeto, não se têm o mesmo conteúdo) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Compara conteúdos (diferencia maiúsculas) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Compara conteúdos (sem diferenciar maiúsculas, só ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Se os conteúdos diferem (o `string/=` do CL. A forma variádica compara pares adjacentes) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Ordem sem diferenciar maiúsculas (o `string-lessp` do CL etc.). Com um prefixo comum, a mais curta é menor |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Uma string com `n` cópias de `c` (o `make-string` do CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | A posição em que `sub` aparece pela primeira vez. **O `search` do CL tem os argumentos ao contrário** (`(search pattern sequence)`). A string vazia é encontrada em 0. Para as palavras-chave, consulte [argumentos de palavra-chave das sequências](sequences.md#6-argumentos-de-palavra-chave) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | A primeira posição em que diferem. `none` só quando são `equal`. Se uma é prefixo da outra, o fim da mais curta. Palavras-chave como acima |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Remove os caracteres contidos em `bag` de ambas as pontas / da esquerda / da direita (o `string-trim` do CL etc.). Sem `bag`, os espaços em branco `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Divide em `sep`. O CL não tem equivalente. Separadores consecutivos produzem elementos vazios. Panic se `sep` estiver vazio |
| `to-string` | `(to-string x)` | `T→string` | Converte para string como faz `~a`. Implementado para `int`/`i32`/`f64`/`bool`/`char`/`string` (o `princ-to-string` do CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Codifica como UTF-8 (cada elemento 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Decodifica. `none` se não for UTF-8 válido |

## 2. Caracteres `char`

Um `char` é um valor escalar Unicode. A conversão de maiúsculas e a classificação tratam só a faixa ASCII.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Converte para maiúscula (só ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Converte para minúscula (só ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Comparação por ponto de código |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Menor estrito por ponto de código (o mesmo que `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Se é uma letra ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Se é um dígito ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Compara valores |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Compara valores sem diferenciar maiúsculas (o `char-equal` do CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Se os valores diferem (o `char/=` do CL. **A forma variádica compara pares adjacentes**, ao contrário do CL, que pergunta se todos os pares diferem) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Ordem sem diferenciar maiúsculas (o `char-lessp` do CL etc.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Maiúscula / minúscula / se tem distinção de maiúsculas (o `upper-case-p` do CL etc.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Uma letra ou um dígito (mesmo nome que no CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Se é imprimível. Inclui o espaço, não a quebra de linha nem a tabulação (o `graphic-char-p` do CL) |
| `standardp` | `(standardp c)` | `char→bool` | Se é um dos 96 caracteres padrão do CL, isto é, `graphicp` mais a quebra de linha (o `standard-char-p` do CL) |
| `char->int` | `(char->int c)` | `char→int` | O valor escalar Unicode (o inverso é `int->char`/`try-int->char` em [Números](numbers.md#1-inteiros-de-largura-fixa)). Corresponde ao `char-code`/`char-int` do CL |
| `char->string` | `(char->string c)` | `char→string` | Uma string de um caractere. A função `string` do CL cobre isso recebendo um designador, mas esta linguagem não tem designadores, então a direção vai no nome |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | O **peso** do dígito naquela base (o `digit-char-p` do CL). `digitp` é outra função que devolve `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | O caractere de peso `w`. Maiúsculas a partir de 10 (o `digit-char` do CL; a base é no máximo 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | O nome do caractere. Só têm nome os caracteres nomeados que o leitor sabe ler (o `char-name` do CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | O caractere de um nome. Não diferencia maiúsculas e também aceita os apelidos do leitor (`linefeed`/`null`) (o `name-char` do CL) |

Não há constante correspondente a `char-code-limit` (o limite superior de `char` é fixado pelo Unicode, não
pela linguagem).

## 3. `Vector<T>`

Um array que pode crescer.
Um valor pode ser escrito `#(1 2 3)` ([referência de sintaxe](../syntax.md#1-elementos-léxicos); o
tipo dos elementos vem do contexto ou do primeiro elemento, e cada avaliação cria um vetor novo).
Ele também é impresso como `#(1 2 3)`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Cria um vetor vazio. O argumento de tipo vem do tipo esperado, então em um `let` simples escreva `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` cópias de `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Acrescenta no final |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Lê o elemento `i`. Panic fora do intervalo |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Muda o elemento `i`. Panic fora do intervalo. Também pode ser escrito `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Número de elementos |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Remove o último elemento e o devolve. `None` se vazio (ao contrário de `get`/`set`, não causa panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Cria um iterador que implementa `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Acrescenta `x` se não existir um elemento igual (o `pushnew` do CL. Não precisa reescrever um lugar, então é um método e não uma macro) |

`map`/`filter` e afins são [funções de sequência](sequences.md#4-funções-de-sequência-sobre-iter): passe o
vetor por `iter`, como em `(map (iter v) f)`. As operações destrutivas (`nreverse`, `delete` etc.) estão em
[Operações destrutivas](sequences.md#7-operações-destrutivas).

## 4. `HashTable<K,V>`

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Cria uma tabela vazia |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Busca |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Inserir ou sobrescrever |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Remove a entrada e devolve o valor antigo, se houver |
| `count` | `(count h)` | `HashTable<K,V>→int` | Número de entradas |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Remove tudo |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Um instantâneo das chaves |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Um instantâneo dos valores |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Um instantâneo dos pares `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Um iterador que implementa `Iter`. Os elementos são `cons-cell` `(k . v)`. Corresponde ao `with-hash-table-iterator` do CL; `doiter`/`map`/`filter` e outros funcionam sobre ele como estão |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | O `maphash` do CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | O `hash-table-size` do CL. Nesta tabela é o número de entradas ocupadas (igual a `count`) |

**Qualquer tipo que implemente `Hash` pode ser chave**, incluindo os tipos `defstruct`/`defenum`.
`get`/`set`/`remove` carregam `(where (Hash K))`, então uma tabela cuja chave é um tipo que não o implementa é
um **erro de tipo** (`f64` não tem `Hash` por causa de `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; devolve um valor não negativo que cabe em um fixnum
```

Implementado para: `int` e os seis inteiros de largura fixa, `bool`, `char`, `string` e `symbol` (não para os
números de ponto flutuante). Nos seus próprios tipos, mantenha o resultado não negativo fazendo `logand` com
`*sxhash-mask*` (2^30-1). Para fazer o hash de uma string, você pode chamar `(sxhash-string s)` (FNV-1a de 32
bits), que é o que a implementação de `string` usa.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Se duas chaves são a mesma é decidido **pelo próprio tipo da chave** (`sxhash`, e `equals` de `Eq`, o
supertrait de `Hash`), não pela identidade do objeto. É por isso que, como acima, dá para buscar com uma
chave que é "um valor diferente, mas igual".

Não há problema se `sxhash` colidir (o contrato de `Hash` vai em uma só direção: valores iguais devem ter o
mesmo hash). As chaves que colidem são distinguidas por `equals`.

## 5. `Array<T>` (arrays multidimensionais)

Um `defstruct` da biblioteca padrão. Não é um tipo embutido, então tudo o que se pode fazer com um
`defstruct` pode ser feito com ele.
Um valor pode ser escrito `#2A((1 2) (3 4))` ([referência de
sintaxe](../syntax.md#1-elementos-léxicos)).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | O `make-array` do CL. `dims` é copiado. `init` é o valor inicial de cada célula (o `:initial-element` do CL; esta linguagem não tem "célula não vinculada", então é obrigatório). `:fill-pointer` só para uma dimensão |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | O `aref` / `(setf (aref …))` do CL. Panic se um índice estiver fora do intervalo |
| `aref` | `(aref a i j …)` | — | A escrita do CL com os índices soltos. Expande para `get`/`set` acima. `(setf (aref a i j) v)` também funciona |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | O `row-major-aref` do CL. Um índice plano |
| `rank` | `(rank a)` | `Array<T>→int` | O `array-rank` do CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | O `array-dimension` do CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | O `array-dimensions` do CL. Devolve uma **cópia**, assim como o CL devolve uma lista nova |
| `total-size` | `(total-size a)` | `Array<T>→int` | O `array-total-size` do CL (o número de células alocadas, independente do ponteiro de preenchimento) |
| `len` | `(len a)` | `Array<T>→int` | O `length` do CL sobre arrays. O ponteiro de preenchimento se houver; senão, `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | O `array-in-bounds-p` do CL. Falso (não um erro) mesmo quando o **número** de índices está errado |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | O `array-row-major-index` do CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | O `adjust-array` do CL. O rank não pode mudar. Os elementos que continuam no intervalo mantêm seus índices, e as células novas recebem `init`. Ao contrário do CL, não devolve o array (todo array desta linguagem é ajustável, então não há um segundo array para devolver) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | O `vector-push-extend` do CL. Panic sem ponteiro de preenchimento |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | O `vector-pop` do CL. `none` se vazio |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | O ponteiro de preenchimento (`none` se não houver). Pode ser escrito com `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Um iterador em ordem por linhas. Para no ponteiro de preenchimento se houver |

- **Os índices são um `Vector<int>`.** Um método não pode declarar "o mesmo tipo de argumento repetido
  qualquer número de vezes no final", e a escrita `aref` cobre essa lacuna.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **não
  existem**. O tipo estático do receptor já responde a essas perguntas.
- `Array::new` é o construtor em ordem de campos gerado por `defstruct` e não serve para criar arrays. Use
  `Array::make`.
- **Os arrays são impressos na sintaxe de arrays do CL.** O rank 1 é `#(1 2 3)`; os outros ranks são `#nA`
  seguido de tantos níveis de parênteses (`#2A((1 2 3) (4 5 6))`); o rank 0 é `#0A5`. A impressão para no
  ponteiro de preenchimento se houver. Definir `*print-array*`
  ([Impressão](printing.md#6-controlar-quanto-é-impresso)) como falso imprime só a forma, `#<array 2x3>`. Só
  um array cujos elementos são um `defstruct` sem `print-object` é impresso na forma embutida
  `#<array<...> ...>` (não é um erro).

## 6. `BitVector` (vetores de bits)

Uma sequência de bits de comprimento fixo. Um `defstruct` da biblioteca padrão.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Comprimento `n`, todos os bits em 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic fora do intervalo |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | As escritas do CL. `(setf (bit v i) b)` também funciona. O `sbit` do CL só difere de `bit` por exigir um vetor de bits simples, mas esta linguagem só tem um tipo de vetor de bits |
| `len` | `(len v)` | `BitVector→int` | Número de bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Devolvem um vetor de bits novo. Panic se os comprimentos diferirem. Não há terceiro argumento como no CL (o destino do resultado) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complemento |

Não existe `bit-vector-p` (o tipo estático responde a isso).

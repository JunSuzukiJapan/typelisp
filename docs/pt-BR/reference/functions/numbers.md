<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Números

Operações sobre inteiros, números de ponto flutuante, racionais, números complexos e booleanos, e outras
funções relacionadas a números. Para ler as formas de chamada, consulte [Funções embutidas](README.md).

## 1. Inteiros de largura fixa

Há sete tipos inteiros: **`int`** (o `integer` do CL: precisão arbitrária, e o tipo padrão dos literais
inteiros sem anotação; capítulo 3), e os de largura fixa `i8` `i16` `i32` `u8` `u16` `u32`. Para qual deles
uma operação é resolvida é decidido pelo tipo do primeiro argumento (são independentes entre si, sem
conversões implícitas). **Não há tipo inteiro de 64 bits.** Um valor em tempo de execução é uma palavra cujos
bits baixos são uma etiqueta, então sobram só 63 bits para um inteiro imediato, e um tipo que dissesse ter 64
bits teria de descartar o bit mais alto em algum lugar. `int` vira bignum ao passar desses 63 bits, então se a
largura não importa, use `int`. A tabela abaixo é para os seis tipos de largura fixa (a tabela de `int` está no
capítulo 3).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | As quatro operações aritméticas. `/` trunca em direção a zero e causa panic na divisão por zero |
| `mod` | `(mod a b)` | `(T,T)→T` | Resto (o `mod` do CL, **divisão por piso**: o sinal segue o divisor. `(mod -7 3)`→`2`). Panic na divisão por zero |
| `rem` | `(rem a b)` | `(T,T)→T` | Resto (o `rem` do CL, **divisão truncada**: o sinal segue o dividendo. `(rem -7 3)`→`-1`). Panic na divisão por zero |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Correspondem a `floor`/`ceiling`/`round`/`truncate` de dois argumentos do CL (`(floor 7 2)`→quociente 3, resto 1). Em vez de valores múltiplos, devolvem o quociente e o resto em um `cons-cell` (`car`=quociente, `cdr`=resto). `round-div` arredonda os empates para o par, como o CL |
| `abs` | `(abs x)` | `T→T` | Valor absoluto |
| `signum` | `(signum x)` | `T→T` | Sinal (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Máximo divisor comum |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Mínimo múltiplo comum (0 se algum for 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | O maior / o menor (três ou mais argumentos são expandidos pela escrita variádica do capítulo 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Comparação |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Todos o mesmo que `=` (não há diferença para números do mesmo tipo) |
| `int->float` | `(int->float x)` | `T→f64` | Conversão de ampliação para `f64` |
| `int->int` | `(int->int x)` | `T→int` | Conversão de ampliação para `int` (sempre exata). O que `(as int x)` faz |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Conversão de ampliação para `ratio` (sempre exata) |
| `int->char` | `(int->char x)` | `T→char` | Interpreta o valor como um valor escalar Unicode. Panic com um valor inválido |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Uma versão de `int->char` que devolve `None` em caso de falha |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Conversão de largura. Os valores que não cabem são truncados (como o `as` do Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | A mesma conversão como pergunta. `None` se o valor não couber nessa largura |

Essas conversões também são o que fazem as formas especiais `(as Type x)`/`(try-as Type x)`
([Referência de sintaxe](../syntax.md#7-outras-formas-especiais)). As operações de bits (`logand`/`ash`/`ldb`
etc.) e os predicados (`zerop`/`evenp` etc.) têm a mesma forma em todos os tipos, então estão reunidos nos
capítulos 11 e 9.

`i8` `i16` `u8` `u16` `u32` têm exatamente a tabela deste capítulo, e `f32` tem exatamente a tabela de `f64` do
capítulo 4.

**Um nome de tipo significa sua largura e seu sinal, nada mais.** `i32` significa "tratar 32 bits como com
sinal" e `u32` significa "tratar 32 bits como sem sinal". `(+ (the u8 200) (the u8 100))` é `44`,
`(+ 2147483647 1)` (como `i32`) é `-2147483648`, e `(lognot (the u32 0))` é `4294967295`. `f32` é igual: um
binary32 de verdade. `(/ (the f32 1.0) (the f32 3.0))` é impresso como `0.33333334`, um valor diferente do
resultado em `f64` `0.3333333333333333`.

O catálogo derivado do CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` e os predicados do capítulo 9) existe
para `int`/`i32`/`f64`/`ratio`. Se precisar dele para outra largura, passe com `(as int x)` / `(as i32 x)`
(existem conversões de largura para todo par).

## 2. Palavras brutas na fronteira com C (`ptr` / `c-long` / `c-ulong`)

Três tipos usados só para passar valores de e para funções C declaradas com
[`defffi`](../syntax.md#33-defffi--declarar-funções-c-ffi). `ptr` é um ponteiro opaco, e `c-long` /
`c-ulong` são o `long` / `unsigned long` do C. Para tornar um deles um valor é preciso estar dentro de
`(unsafe ...)`.

**Não há aritmética.** Nada da tabela do capítulo 1 se aplica: não se pode escrever nem `(+ p 1)` nem
`(< n m)`. São palavras para entregar ao C, não tipos para calcular, então para calcular passe a um tipo com
largura. `c-long` / `c-ulong` só têm conversões:

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | As mesmas conversões de largura do capítulo 1. Os valores que não cabem são truncados |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | A mesma conversão como pergunta |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | O caminho de entrada, a partir da outra palavra bruta e dos tipos inteiros do capítulo 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Igual ao acima |
| `int->int` | `(int->int x)` | `T→int` | **Sempre exata**. A forma honesta de ler um `size_t` que não cabe em um `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` é o que estas fazem, e as conversões existem
para todo par com os tipos inteiros do capítulo 1. `ptr` nem sequer tem esta tabela: não se oferece nenhuma
forma de ler um ponteiro como número. É um valor que só é passado, recebido e entregue a outra função C.

**Também não podem ser impressas.** `(println "~a" x)` não aceita uma palavra bruta (ela não tem
representação `Sexpr`), então primeiro passe-a a um tipo com largura, como em `(println "~a" (as int n))`.

"Não há tipo inteiro de 64 bits", do início do capítulo 1, vale também para estes três. Vale **porque eles não
podem ser armazenados**: não podem ser um campo de `defstruct`, um `defvar`, nem ficar dentro de um argumento
de tipo ou de um `Sexpr`, então são palavras que só atravessam uma função como argumentos, valores de retorno e
variáveis locais. Para os detalhes, consulte a
[Referência de sintaxe](../syntax.md#ptr--c-long--c-ulong--palavras-de-máquina-brutas).

## 3. Inteiros de precisão arbitrária `int`

O `integer` do CL, e o **inteiro** desta linguagem: os literais inteiros sem anotação têm este tipo, e as
funções embutidas que devolvem um número, como `length` e `char->int`, devolvem este tipo. Um valor é guardado
como valor imediato de 63 bits (fixnum) enquanto cabe, é promovido automaticamente a bignum quando o resultado
de uma operação deixa de caber, e volta a ser imediato quando cabe de novo. `eq` é sempre identidade de valor
dentro da faixa fixnum, e `eql`/`=` são identidade numérica em toda a faixa. É um tipo diferente dos tipos
inteiros de largura fixa (capítulo 1), sem conversão implícita: `(as int x)` é a ampliação exata a partir de
uma largura fixa, e `(as i32 n)` / `(try-as i32 n)` são o truncamento / a verificação a partir de `int` (o
mesmo significado de `int->W` / `try-int->W` do capítulo 1).

A variante inteira de `Sexpr` também é simplesmente `int` (`(int n)` aceita tanto fixnums quanto bignums).

As funções embutidas que recebem um índice ou uma contagem (`substring`, `get` de `Vector`, a quantidade de
deslocamento de `ash` etc.) aceitam `int`, mas passar um valor que não cabe em um fixnum é um erro em tempo de
execução ("an integer argument does not fit a fixnum").

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Nunca transbordam (promovem) |
| `/` | `(/ a b)` | `(int,int)→int` | Trunca em direção a zero. Panic na divisão por zero |
| `mod` | `(mod a b)` | `(int,int)→int` | Resto da divisão por piso (o sinal segue o divisor) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Todos `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Iguais aos do capítulo 11 (complemento de dois com infinitos bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Iguais aos do capítulo 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Truncamento / verificação. `W` é uma das seis larguras ou `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identidade (no lado de largura fixa e das palavras C, `int->int` amplia; capítulo 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | A mesma forma do capítulo 1. `expt` só aceita expoentes não negativos |

## 4. Números de ponto flutuante (`f64` / `f32`)

`f32` tem a mesma tabela.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. A divisão por zero não causa panic; dá `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Resto da divisão por piso (como no CL; o sinal segue o divisor. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Resto da divisão truncada (como no CL; o sinal segue o dividendo. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Comparação |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Todos o mesmo que `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potência |
| `abs` | `(abs x)` | `f64→f64` | Valor absoluto |
| `signum` | `(signum x)` | `f64→f64` | Sinal (`1.0`/`-1.0`; `±0.0`/`NaN` são devolvidos como estão. Como no CL, ao contrário do `signum` do Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | O maior / o menor (três ou mais argumentos são expandidos pela escrita variádica do capítulo 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Operações unárias |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Funções transcendentes. `log` é o logaritmo natural |
| `log` (dois argumentos) | `(log x base)` | `(f64,f64)→f64` | Logaritmo em uma base dada. Expandido para `(/ (log x) (log base))` (capítulo 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Correspondem às versões de dois argumentos do CL (`(floor 7.0 2.0)`→quociente 3, resto 1). O mesmo desenho das funções homônimas do capítulo 1 (`car`=quociente, `cdr`=resto) |
| `float->int` | `(float->int x)` | `f64→int` | Converte para `int` truncando em direção a zero (o `truncate` do CL; exato para valores finitos de qualquer tamanho). Panic com infinito e NaN. Para uma largura fixa, use `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Converte para `ratio` como o racional binário exato (o `rational` do CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Converte entre larguras de ponto flutuante. `float->f32` arredonda para o mais próximo, `float->f64` é sempre exata. O que `(as f32 x)` faz |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | A mesma conversão como pergunta. `none` se o arredondamento mudar o valor (ampliar para `f64` é sempre `some`). O que `(try-as f32 x)` faz |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | As funções homônimas do CL. Apelidos de `floor`/`ceiling`/`round`/`truncate` acima: no CL as sem prefixo devolvem inteiros, então as com `f` batem com o comportamento desta linguagem |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 2 / 53 / 53 respectivamente (só a precisão de `0.0` é 0). `f64` é sempre IEEE-754 binary64, então são constantes |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` ou `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | A mantissa (em `[1/2,1)`, sem sinal) e o expoente. O CL devolve três valores, mas não há valores múltiplos, então o sinal fica a cargo de `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | A mesma decomposição com uma mantissa inteira exata de 53 bits. `mantissa * 2^expoente` é exatamente o valor original |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **O racional mais simples que é lido de volta como esse número de ponto flutuante** (`(rationalize 0.1)` é `1/10`). Para o valor binário exato, use `float->ratio` |

**Diferença em relação ao CL: como `round` arredonda.** `round` (e portanto `fround`/`round-div`) arredonda
**afastando-se de zero** (`(round 2.5)` = `3.0`). O CL arredonda **para o par**, dando `2`.

## 5. Racionais `ratio`

Racionais de precisão arbitrária compatíveis com o CL. São sempre mantidos na forma irredutível com
denominador positivo, e alocados no heap. Não há conversão implícita com os tipos inteiros nem com `f64` (use
um método de conversão explícito ou `as`/`try-as`). Para a sintaxe dos literais de ratio, consulte a
[Referência de sintaxe](../syntax.md#1-elementos-léxicos).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | As quatro operações (resultados sempre irredutíveis). `/` causa panic na divisão por zero |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Resto da divisão por piso (como no CL; o sinal segue o divisor) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Resto da divisão truncada (como no CL; o sinal segue o dividendo) |
| `abs` | `(abs x)` | `ratio→ratio` | Valor absoluto |
| `signum` | `(signum x)` | `ratio→ratio` | Sinal (devolve `1`/`-1`/`0` como `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potência. O expoente deve ser um `ratio` de valor inteiro (senão, panic). Um expoente negativo dá o recíproco |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | O maior / o menor |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` não tem operações de bits (no CL elas são só para inteiros) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Comparação |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Todos o mesmo que `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Numerador na forma irredutível (mesmo nome que no CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Denominador na forma irredutível (sempre positivo) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Parte inteira (truncada em direção a zero) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Converte para `f64` |

Os caminhos de entrada a partir dos inteiros de largura fixa e de `f64` são `int->int`/`int->ratio`
(capítulo 1) e `float->int`/`float->ratio` (capítulo 4). `int`/`ratio` são tipos separados, independentes de
`i32` e dos outros, e a aritmética mista precisa de conversões explícitas.

## 6. Números complexos `complex`

Uma estrutura (`defstruct`) da biblioteca padrão.

**Duas diferenças em relação ao CL** (ambas decorrem da tipagem estática):

1. **Os componentes são sempre `f64`.** Um complexo do CL também pode conter racionais, e `(complex 1 2)` e
   `(complex 1.0 2.0)` são tipos diferentes. Um tipo estático tem de escolher um, e as funções transcendentes
   devolvem o tipo de ponto flutuante.
2. **`(sqrt -1.0)` é o `sqrt` real (NaN).** No CL, `sqrt` pode devolver um complexo a partir de um real, mas o
   `sqrt` de `f64` tem de devolver um `f64`. Um resultado complexo vem de um argumento complexo:
   `(sqrt (complex -1.0 0.0))` é `i`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Construção. Os componentes podem ser lidos diretamente como `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Parte real e parte imaginária. **Também funcionam com números reais** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), como no CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Conjugado (também funciona com reais) |
| `phase` | `(phase z)` | `complex→f64` | Argumento em (-pi,pi] (também funciona com reais) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Valor absoluto. **O único `abs` que não devolve o tipo do receptor** (como no CL, o valor absoluto de um complexo é real) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Aritmética complexa |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Igualdade componente a componente. `Eq` também é implementado (não há `Ord`: complexos não têm ordem, e o `<` do CL também os rejeita) |
| `zerop` | `(zerop z)` | `complex→bool` | Se ambos os componentes são 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` dão valores principais |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | O ângulo do vetor `(x,y)`. **O `(atan y x)` de dois argumentos do CL é uma escrita disto** (ramifica conforme o número de argumentos, como o `log` de dois argumentos) |

Implementa `print-object`, então `~a`/`~s` o imprimem como `#C(re im)`, como o CL faz (o leitor desta linguagem
não tem a sintaxe `#C` para lê-lo de volta).

## 7. Booleanos

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negação |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Todos comparam a igualdade dos valores |

`and`/`or` precisam de avaliação em curto-circuito, então são formas especiais
([Referência de sintaxe](../syntax.md#4-vínculos-e-condicionais)).

## 8. Auxiliares numéricos e escritas de chamada

`abs`/`signum` (todos os tipos numéricos), `gcd`/`lcm` (só tipos inteiros), `rem` (todos os tipos reais,
incluindo `f64`) e `expt` (`int`/`f64`/`ratio`) são definidos como métodos de cada tipo numérico (resolvidos
pelo tipo do receptor: `(abs x)` é o método do tipo de `x`). Os detalhes de cada tipo estão nos capítulos 1, 3,
4 e 5. Os inteiros de largura fixa não têm `expt` (não têm promoção e transbordariam; passe para `int` com
`(as int x)` e use o `expt` dele).

### 8.1 Formas variádicas e de 0/1 argumento

A aritmética e a comparação do CL são variádicas, mas os métodos são resolvidos só pelo tipo do receptor, não
pelo número de argumentos. Então **o verificador expande as formas a seguir em chamadas de dois argumentos**.

| Forma que você pode escrever | Expansão | Aplica-se a |
|---|---|---|
| `(op a b c ...)` | A dobra à esquerda `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` com cada termo vinculado a um temporário | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Os acima que têm elemento neutro |
| `(op x)` | Para `+ * max min logand logior logxor`, o próprio `x`. `(- x)` troca o sinal, `(/ x)` dá o recíproco, `(gcd x)`/`(lcm x)` dão `(abs x)` (como no CL) | Igual ao acima |
| `(cmp x)` | Avalia `x` e dá `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Cada termo é avaliado exatamente uma vez, da esquerda para a direita (é por isso que as comparações variádicas
passam por temporários). A forma variádica de `/=` compara **pares adjacentes**, ao contrário do CL, que
pergunta se todos os pares diferem.

### 8.2 `isqrt` e `expt` inteiro

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | O maior inteiro que não passa da raiz quadrada. Panic com um valor negativo |
| `expt` | `(expt n e)` | `(T,T)→T` | Potência (por quadrados). O CL devolve um racional para um expoente negativo, mas um tipo inteiro não pode representá-lo, então ocorre panic; converta primeiro para `ratio` |

## 9. Predicados

| Nome | Forma | Tipo | Tipos |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (só tipos inteiros, como no CL) |

**Não há predicados de tipo** como `numberp`/`integerp`/`floatp` do CL. Com tipagem estática, o tipo de um
valor já está definido sem perguntar em tempo de execução.

## 10. Constantes

| Nome | Tipo | Valor |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Códigos de operação passados a `boole` (no lugar das palavras-chave do CL) |

Constantes de limites numéricos (CLHS 12.1.4.2 / 12.1.3):

| Nome | Tipo | Descrição |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | O limite superior / inferior de um valor imediato de 63 bits (2^62-1 / -2^62). Um `int` além deles vira bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Os maiores / menores valores finitos |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | A menor magnitude não nula, incluindo os subnormais |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | O mesmo, limitado a números normalizados |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Seguem a definição do CL (o menor `e` positivo com `(/= (+ 1 e) 1)`), então são **um ULP maiores que** 2^-53: o próprio 2^-53 volta a `1.0` com arredondamento para o par mais próximo |

## 11. Operações de bits

Definidas sobre complemento de dois com infinitos bits (CL 12.10). São implementadas para os tipos inteiros de
largura fixa e para `int`, não para `ratio` (o CL também tem operações de bits só para inteiros).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | E, OU e OU exclusivo bit a bit (versões variádicas e de zero argumentos em 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Complemento bit a bit |
| `ash` | `(ash x count)` | `(T,int)→T` | Deslocamento aritmético. Para a esquerda se `count` for positivo, para a direita se for negativo |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Se o bit `index` está ligado (**a ordem dos argumentos é a inversa do CL**; veja abaixo) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | O número de bits ligados (para um número negativo, o número de bits 0) |
| `integer-length` | `(integer-length x)` | `T→T` | O número de bits necessários para representá-lo, sem contar o sinal |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Os sete restantes, compostos a partir dos anteriores |

**Só o segundo argumento de `ash` é `int` em vez de `T`.** É uma **distância** em bits, não um valor do tipo do
receptor, então a largura e o sinal do receptor não dizem nada sobre a distância (pelo mesmo motivo de `count`
no `(ash integer count)` do CL ser qualquer inteiro). Deslocar para a direita um valor sem sinal é um
deslocamento lógico (`(ash (the u8 200) -3)` = `25`), e um com sinal é um deslocamento aritmético que arredonda
em direção a menos infinito (`(ash (the i32 -100) -4)` = `-7`). O `index` de `logbitp` é `int` pelo mesmo
motivo.

**Especificadores de byte.** Em vez do objeto opaco que o `byte` do CL devolve, usa-se um `cons-cell<int,int>`
(`car`=tamanho, `cdr`=posição). Tanto o tamanho quanto a posição são números de bits, então são `int`
qualquer que seja a largura do inteiro sendo desmontado.

**O inteiro é o primeiro argumento, em uma ordem diferente da do CL.** O CL escreve `(ldb bytespec integer)`,
mas esta linguagem escolhe um método pelo tipo do receptor (o primeiro argumento), e com o especificador
primeiro não poderia escolher pelo tipo do inteiro. Todas as outras operações de bits têm a forma
`(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), e só a família `ldb` e `logbitp` eram ao
contrário, então foram alinhadas. Os argumentos restantes mantêm a ordem relativa do CL, então
`(dpb newbyte spec n)` vira `(dpb n newbyte spec)`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Cria um especificador de byte |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Tira um componente |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extrai de `x` o byte especificado, alinhado à direita |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Se algum bit do byte especificado está ligado |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Zera tudo fora do byte especificado (mantendo as posições) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Deposita o `newbyte` alinhado à direita no byte especificado de `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | A versão de `dpb` que mantém as posições |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Uma das 16 operações lógicas de dois operandos, escolhida por `op` (uma constante `boole-*` do capítulo 10) |

`T` é um tipo que implementa o trait `Bits`, isto é, `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Só `boole` mantém
`op` primeiro, já que ali não há motivo para mudar a ordem do CL.

## 12. Números aleatórios

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Um número aleatório de `0` até `n`, sem incluí-lo. Se o estado for omitido, tira de `*random-state*` e o faz avançar |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Sem argumento, um estado novo; com um, uma cópia dele (a cópia reproduz a mesma sequência) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Sempre `true` (o tipo estático já descarta outros tipos; existe só para corresponder ao CL) |
| `*random-state*` | — | `random-state` | O estado padrão de `random`. Uma global que pode ser atribuída (substitua-a com `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | O estado que o inteiro designa. A mesma semente sempre reproduz a mesma sequência |

O gerador é xorshift64 e devolve a mesma sequência tanto interpretado quanto compilado.

Um estado novo de `make-random-state` é semeado a partir do relógio de parede, então não pode ser reproduzido
entre execuções. Para reproduzir, use `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; imprime os mesmos três números em toda execução
```

**O CL não tem uma forma portável de dar uma semente** (`make-random-state` só aceita `nil`/`t`/um estado),
então este nome segue o `sb-ext:seed-random-state` do SBCL, e não o CL.

Sementes diferentes dão sequências diferentes. `(seed-random-state 0)` e `(seed-random-state 1)` dão
sequências diferentes, assim como `-7` e `7`.

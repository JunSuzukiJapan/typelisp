<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# Referência de sintaxe do typelisp

typelisp é um Lisp com tipagem estática, escrito em expressões S. Para a lista de funções e métodos embutidos,
consulte [Funções embutidas](functions/README.md); para a lista de tipos, [types.md](types.md); e para ler as
mensagens de erro, [errors.md](errors.md).

## 1. Elementos léxicos

- **Não diferencia maiúsculas e minúsculas.** Os símbolos são todos normalizados para minúsculas ao serem lidos.
- **Comentários**: de `;` até o fim da linha (comentários de linha). `#| ... |#` (comentários de bloco, que
  podem ser aninhados).
- **Avaliação em tempo de leitura**: `#.(expr)` **executa a forma seguinte durante a leitura** e trata seu
  valor como o que foi lido. É o único lugar em que o leitor é mais do que uma função do texto. Até onde ele
  pode chegar depende do caminho de leitura, como no CL:
  - `(load ...)` e o REPL avaliam uma forma por vez, então ele pode chamar **funções definidas antes no mesmo
    texto** (o `load` do CL).
  - Um arquivo de módulo é verificado como uma unidade e executado por quem o usa com `use`, então `#.` só
    alcança a biblioteca padrão e o que a sessão já executou. Nem as definições do próprio arquivo nem as dos
    módulos que ele usa **foram executadas ainda** (assim como o `compile-file` do CL precisa de `eval-when`).
  - `read` / `read-from-string` dentro de um programa também avaliam `#.` (como no CL).
  - Definir `*read-eval*` (padrão `true`) como `false` faz de `#.` um erro de leitura em todo lugar: um
    interruptor para impedir que um texto lido como dados execute código (como no CL). Ele é consultado a cada
    `#.`, então um `setf` passa a valer a partir da próxima forma lida. Dentro de `with-standard-io-syntax` ele é
    `true`.
- **Booleanos**: `true` / `false`.
- **Inteiros**: decimais (`42`, `-7`). Um sinal `+`/`-` pode vir primeiro. As outras bases são escritas com a
  sintaxe de base do CL `#b`/`#o`/`#x`/`#NNr` (o sinal vai depois da marca: `#x-ff`). O prefixo `0x` não existe
  no CL e não é adotado: `0xff` é lido como um símbolo.
  Um literal inteiro sem anotação de tipo é `int` por padrão (precisão arbitrária,
  [Números](functions/numbers.md#3-inteiros-de-precisão-arbitrária-int)), sem limite superior de tamanho. **Se o
  tipo esperado for um tipo inteiro de largura fixa, o literal assume esse tipo, e verifica-se se o tipo
  consegue conter o valor**: `(the u8 300)` é um erro de tipo (se você quiser recortá-lo, escreva
  `(as u8 300)`). `(the u32 4294967295)` e `(the u32 #xFFFFFFFF)` podem ser escritos graças a essa regra. Se um
  valor `int` cabe em um imediato de 63 bits ou vira bignum é decidido pelo seu tamanho, sem sintaxe especial
  (como no CL).
- **Números de ponto flutuante**: os que contêm um ponto decimal ou um expoente (`e`/`E`) (`1.5`, `3.0e10`).
  `f64` por padrão (`f32` se esse for o tipo esperado).
- **Ratios**: `numerador/denominador` (só decimal, por exemplo `1/3`). Reduzidos ao serem lidos, como o CL
  especifica (`2/4` é `1/2`). Os que têm valor inteiro (`4/2` etc.) são lidos como `int`, não como `ratio`. Um
  denominador zero (`1/0`) é um erro de leitura.
- **Caracteres**: `#\` seguido de um caractere ou de um nome de caractere. Por exemplo `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (também `#\Null`) `#\Backspace`. Os nomes não diferenciam
  maiúsculas.
- **Strings**: `"..."`. Os escapes são `\n` `\t` `\r` `\0` `\\` `\"` (qualquer outro `\x` é simplesmente `x`).
- **Símbolos**: qualquer token contendo letras, dígitos e símbolos (`+` `<=` `my-func` etc.).
  `]` e `}` terminam um token, então não podem aparecer dentro de um símbolo, e encontrar um deles
  no início de um dado é um erro de leitura. `[` e `{` podem aparecer dentro de um símbolo: como em
  CL, ficam livres para o programador usar em [macros de leitura](#11-macros-de-leitura-readtable).
- **Palavras-chave**: símbolos que começam com dois-pontos, como `:name` (como no CL). São autoavaliadas: não
  procuram vínculo nenhum e seu valor são elas mesmas, com tipo estático `symbol`. Palavras-chave com o mesmo
  nome são sempre o mesmo objeto (`(eq :foo :FOO)` é verdadeiro; como os outros símbolos, viram minúsculas). Os
  próprios dois-pontos fazem parte do nome, então `(symbol->string :foo)` é `":foo"` (o typelisp não tem sistema
  de pacotes, então isso difere do `symbol-name` do CL). Dois-pontos sozinhos `:` ou com dois-pontos adicionais
  como `:a:b` são um erro de leitura. Teste com `keywordp`. As que começam com `::` não são palavras-chave, mas
  caminhos absolutos (abaixo).
  Note que `:dyn` é uma palavra-chave reservada só para posições de tipo; escrevê-la em qualquer outro lugar é um
  erro (veja o [capítulo 2](#2-escrita-de-tipos)).
- **Listas**: `(a b c)`. Pares pontuados `(a . b)` também podem ser lidos.
- **Vetores**: `#(1 2 3)` (como em CL). O conteúdo é composto só de literais e não é avaliado: o `a`
  de `#(a b)` é um símbolo, não uma variável. O tipo dos elementos vem do contexto
  (`(the Vector<i32> #(1 2))`), ou do primeiro elemento quando não há contexto (`#(1 2 3)` é um
  `Vector<int>`). Todos os elementos devem ter o mesmo tipo: `#(1 "a")` é um erro de tipo, assim
  como `#()` sem elementos nem contexto. Cada avaliação cria um vetor novo. Onde se esperam dados de
  expressões S (`(the Option<Sexpr> #(1 x))`, `'#(..)`, o que `read` devolve), é um
  `Vector<Option<Sexpr>>` cujos elementos são todos dados: a variante `vector` de `Sexpr`.
- **Arrays**: `#2A((1 2) (3 4))` (como em CL). O número entre `#` e `A` é o posto, e essa mesma
  quantidade de primeiros níveis de aninhamento de listas no conteúdo são as dimensões. `#0A x` é um
  array de dimensão zero que contém um elemento. Listas do mesmo nível com comprimentos diferentes
  são um erro de leitura. O tipo é decidido como nos vetores e é um `Array<T>` (sem elementos, o
  contexto precisa fornecê-lo, como em `(the Array<f64> #2A(()))`). Como dado de expressões S é um
  `Array<Option<Sexpr>>`: a variante `array` de `Sexpr`.
- **A lista vazia `()`**: conforme o contexto, o valor do tipo `Unit` ou o `none` de `Option<Sexpr>`.
  **`Sexpr` não tem variante de lista vazia**: `Sexpr` significa "uma expressão S não vazia", e o tipo dos dados
  de expressões S é `Option<Sexpr>` (veja "Padrões para `Option<Sexpr>`" em
  [4.3 match](#43-match--casamento-de-padrões)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (só tem sentido dentro de um quasiquote)
  - `,@x` → `(unquote-splicing x)` (encaixado como elementos de lista na expansão)
- **Caminhos `::`**: `foo::bar` é lido como um caminho através de módulos, tipos e membros (não como um único
  nome de símbolo). Um que começa com `::`, como `::foo`, é um caminho absoluto a partir da raiz. Um `::` dentro
  de argumentos genéricos (`Vec<a::b>` e afins) não é tratado como separador de caminho.

## 2. Escrita de tipos

No código-fonte, os tipos são escritos como símbolos ou listas comuns.

- **Tipos primitivos**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`.
  `int` é o tipo inteiro (o integer do CL, que passa automaticamente entre imediatos de 63 bits e bignums;
  [Números](functions/numbers.md#3-inteiros-de-precisão-arbitrária-int)), e os seis tipos de largura fixa são
  nomeados pela largura e pelo sinal (não há tipo inteiro de 64 bits; veja
  [Números](functions/numbers.md#1-inteiros-de-largura-fixa)).
- **O tipo racional**: `ratio` (racionais na forma irredutível). Alocados no heap como no CL, sem conversão
  implícita com `int`/`f64` e afins (converta explicitamente com `as`/`try-as` ou com um método de conversão;
  veja [Números](functions/numbers.md#5-racionais-ratio)).
- **Palavras brutas na fronteira com C**: `ptr` (um ponteiro opaco), `c-long` / `c-ulong`. Só para o FFI:
  tornar uma delas um valor exige `(unsafe ...)`, e os lugares onde podem aparecer são limitados
  ([3.3 defffi](#ptr--c-long--c-ulong--palavras-de-máquina-brutas)). Não as use onde você quer um inteiro de
  64 bits: elas não têm aritmética.
- **Tipos mutáveis opacos**: `random-state` (o estado de um gerador de números aleatórios). Não pode ir em
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (pode ir em `Option<T>`/`Result<T,E>`).
- **O tipo Unit**: `()`
- **O tipo Never**: `!` (o tipo das expressões que divergem, como `panic`/`unreachable`/`todo`/um laço que nunca
  retorna. Encaixa-se em qualquer tipo esperado)
- **Tipos de função**: `(fn (tipos-dos-argumentos...) tipo-de-retorno)`. O tipo de uma função com argumentos
  variádicos é `(fn (tipos-dos-argumentos... &rest tipo-do-elemento) tipo-de-retorno)`.
- **Tipos genéricos**: `Name<T1,T2,...>` (lido como um único token sem espaços).
  Por exemplo `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  O tipo unit `()` também pode ser escrito como argumento de tipo (`Result<(), FileError>`). `(`/`)` são
  normalmente delimitadores que terminam um token, mas enquanto um sinal de menor está aberto, esse par de
  caracteres é deixado passar. `()` também pode ser usado como tipo de campo ou de argumento.
- **A forma de aplicação dos tipos genéricos**: `(Name T1 T2 ...)`, uma escrita em forma de lista que nomeia o
  mesmo tipo que `Name<T1,T2,...>`. Por exemplo `(vector char)` é o mesmo que `Vector<char>`.
  A forma com nome é a habitual; esta forma **existe para quando um argumento de tipo não pode ser escrito
  dentro de um nome**: um argumento de tipo é ele mesmo uma expressão de tipo, mas dentro de um nome de um único
  token só se podem escrever nomes, `()` e `:dyn`, não tipos de função (não existe uma escrita como
  `Vector<(fn (i32) i32)>`). Também pode aparecer nesta forma quando a implementação mostra um tipo, como o
  resultado de substituir o tipo associado de um trait em uma assinatura.
- **Nomes de tipo qualificados**: podem ser qualificados com `::`, como em `module::Type`.
- **Tipos de objeto trait**: `:dyn Trait` (duas palavras separadas por espaço formando um tipo). Representa um
  valor cujo tipo concreto é decidido em tempo de execução; as chamadas a métodos de trait passam por uma vtable
  (despacho dinâmico). Para um trait com tipos associados, eles são fixados por posição na ordem de declaração
  (`:dyn Iter<i32>` fixa `Item` em `i32`). Também pode ser escrito dentro de argumentos genéricos:
  `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Os valores concretos são encaixotados
  automaticamente nas posições esperadas; a forma explícita é `(as :dyn Trait expr)`.
  Um valor de `:dyn Sub` pode ser passado como está onde se exige um `:dyn Super` de qualquer um dos seus
  supertraits (tudo o que ele herda, transitivamente) (upcast). Não pode ser passado a um trait não
  relacionado. Para as condições que um trait precisa cumprir para ser usado com `:dyn`, veja
  [3.9 deftrait / impl](#39-deftrait--impl--traits). Escrever `:dyn` fora de uma posição de tipo é um erro.
- Tipos genéricos embutidos: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, e os tipos de concorrência `Task<T>` / `Thread<T>` / `Chan<T>`
  ([capítulo 12](#12-concorrência-tarefas)). Também há `Sexpr`, o tipo dos dados de expressões S. Os tipos de erro
  concretos embutidos são `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` /
  `NetError`, e a biblioteca padrão tem as estruturas `SimpleError` / `WrappedError` (`Error` não é um tipo, mas
  um trait: use-o como `:dyn Error`). A lista está em [types.md](types.md).
- **Tipos e traits compartilham um namespace** (como no Rust): dentro de um módulo, um tipo
  (`defstruct`/`defenum`) e um trait (`deftrait`) não podem ter o mesmo nome.

## 3. Definições de nível superior

### 3.1 defun — definição de funções

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Os tipos dos argumentos e o tipo de retorno são obrigatórios.
- Uma função genérica escreve seus parâmetros de tipo entre sinais de menor e maior depois do nome:
  `(defun name<T1,T2...> (params) Ret body...)` (a mesma sintaxe de `Vector<T>` nas posições de tipo).
- `defun`/`lambda`/`defmethod` aceitam argumentos variádicos quando `&rest (name Type)` é escrito no final:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (no corpo, `xs` é sempre vinculado como um
  `Option<Sexpr>`, uma lista de expressões S. Cada argumento real da chamada tem o tipo verificado
  individualmente como `Type2` e depois é envolvido em um `Sexpr`).
  `defmacro` também tem seu próprio `&rest`, mas difere por ser sempre um `Sexpr` sem tipo (`defun`/`lambda`
  declaram o tipo dos elementos). Um tipo de função também pode descrever uma função variádica, como
  `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (para `defun` e `defmethod`; não para `lambda`/`labels`, pelo motivo abaixo, e
  `defmacro` tem uma implementação separada, também abaixo). A ordem é a do CL:
  `obrigatórios &optional &rest &key`. Cada parâmetro é escrito `(name Type)` ou
  `(name Type expr-padrão)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; sem padrão
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> no corpo

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; com padrão
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; quem chama escreve `:name valor`, em qualquer ordem; os omitidos assumem seus padrões
  ```

  - **Um parâmetro sem expressão padrão tem o tipo `Option<Type>`.** Omitido, é `none`; passado, o valor simples
    que quem chama escreveu é envolvido automaticamente em `some`. O que o CL faz com uma variável supplied-p
    ("foi fornecido?") aparece aqui do lado do tipo estático.
  - Com uma expressão padrão, o tipo continua `Type` como declarado. Quando omitido, essa **expressão
    verificada** é embutida na chamada como está (avaliada a cada chamada).
  - **`&key` não pode ser misturado com `&optional`/`&rest` em uma mesma lista de argumentos.** Isso evita uma
    ambiguidade que o próprio CL tem (se um argumento real final é tomado por um `&optional` posicional ou
    casado por rótulo como `&key` depende dos *valores*), proibindo a combinação. `&optional` e `&rest` podem ser
    usados juntos.
  - Podem ser usados em funções genéricas, mas **um parâmetro de tipo que só aparece em argumentos omitidos não
    pode ser inferido e é um erro** (não há valor com que casá-lo).
  - **`defmethod` pode ter as mesmas três seções** (tanto para métodos de instância quanto para funções
    estáticas). Liste `&optional`/`&rest`/`&key` depois do receptor:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; função estática
    (point::origin :y 7)
    ```

    Também podem ser usados em métodos de tipos genéricos, mas **o tipo de um parâmetro com expressão padrão não
    pode mencionar os parâmetros de tipo do dono** (a mesma restrição que `defun` tem para seus próprios
    parâmetros de tipo: o que é embutido quando o argumento é omitido é uma expressão *verificada*, então seu
    tipo não pode ficar como uma variável abstrata).
  - **Não podem ser usados em métodos de trait.** `deftrait` não tem sintaxe para eles, e se só o lado do `impl`
    pudesse declarar seções, as chamadas com um receptor `:dyn` (que preenchem os argumentos a partir da
    declaração do trait) e as chamadas com um receptor concreto (que os preenchem a partir da declaração do
    `impl`) se tornariam coisas diferentes. A aridade de um slot de vtable é fixa.
  - **Não podem ser usados em `lambda` / `labels`** (`&rest` pode). Para preencher um argumento omitido, quem
    chama precisa ler **a expressão padrão verificada de quem é chamado**, que só está disponível a partir de
    uma assinatura resolvida pelo nome. Uma `lambda` é passada como valor, e a única coisa que descreve esse
    valor é seu tipo de função `(fn ...)`: nele não há lugar para uma expressão, e se houvesse, "duas lambdas
    com a mesma assinatura mas padrões diferentes" se tornariam tipos diferentes. `&rest` fica no terreno dos
    tipos, então pode ser escrito em um tipo de função.
- **As referências antecipadas são declaradas com `defsignature`** (abaixo). Um nome que não foi declarado não
  pode ser chamado antes da sua definição, porque o nível superior é verificado e executado uma forma por vez, na
  ordem do código-fonte.
- Para exigir restrições de trait, escreva uma cláusula `where` logo antes do corpo:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (fixar um tipo associado com `(AssocName ConcreteType)` é opcional).
- **Docstrings**: um literal de string no início do corpo, logo depois da cláusula `where` (se houver), vira a
  docstring (como no CL). Mas só quando pelo menos uma forma do corpo o segue: uma string sozinha continua sendo
  o valor de retorno e não é tomada como docstring: `(defun f () string "doc" "value")` tem docstring e devolve
  `"value"`, enquanto `(defun f () string "value")` não tem docstring e devolve `"value"`. Ela pode ser
  recuperada com `(documentation name)` ([docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — declarações antecipadas

```lisp
(defsignature name (tipos-dos-argumentos...) tipo-de-retorno)
(pub defsignature name (tipos-dos-argumentos...) tipo-de-retorno)
```

Para chamar um `defun` definido **depois** de você, declare-o primeiro assim. A recursão mútua só pode ser
escrita deste jeito:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Os argumentos são listados **só pelos tipos**; não há corpo, então não há nada a que dar nomes. `&rest` pode ser
escrito por último, como `&rest tipo-do-elemento`.

As declarações **são verificadas**:

- A definição que segue precisa bater com a declaração (o número e os tipos dos argumentos, o tipo de retorno,
  `&rest` e se é `pub`). Uma divergência é um erro na definição.
- Declarar sem definir é um erro (relatado quando o arquivo / módulo termina de carregar). O REPL não o relata
  depois de cada entrada, porque uma declaração e sua definição devem poder ser digitadas em linhas separadas.
- Uma declaração colocada **depois** da definição é um erro, já que tal declaração não poderia fazer nada.

Três coisas não podem ser declaradas:

- **Funções genéricas.** Fazer uma cópia para cada tipo exige o corpo, e uma declaração não tem. Uma chamada
  antecipada poderia ser resolvida, mas a instanciação falharia, então a declaração é recusada de saída.
- **`&optional`/`&key`.** Sua assinatura inclui a expressão **verificada** de cada valor padrão (embutida na
  chamada quando o argumento é omitido), e uma declaração não tem lugar para ela.
- **Qualquer coisa que não seja `defun`.** Um `defmacro` precisa que o corpo da macro **já tenha sido executado**
  para expandir, o que registrar uma assinatura não pode substituir. Para os tipos (`defstruct`/`defenum`/
  `deftrait`), registrá-los é "o que o próprio código que registra o tipo precisa", o que não é autocontido como
  uma assinatura. Um `defmethod` é registrado no tipo que o possui, então segue o tipo.

O equivalente no CL é `(declaim (ftype (function (i32) bool) even2))`, mas isso vem com todo um sistema de
declarações e é só **orientativo**. Aqui, com tipagem estática, as declarações são verificadas.

### 3.3 defffi — declarar funções C (FFI)

```lisp
(defffi (nome "c_symbol") (tipos-dos-argumentos...) tipo-de-retorno)
(defffi (nome "c_symbol") (tipos-dos-argumentos...) tipo-de-retorno :library "nome")
(defffi nome (tipos-dos-argumentos...) tipo-de-retorno)              ; nome = o nome do símbolo em C
(pub defffi ...)
```

Declara uma função C para que possa ser chamada. A forma é a mesma de `defsignature` (um nome, tipos de
argumento, um tipo de retorno e sem corpo), mas não ter corpo significa outra coisa. `defsignature` é uma
promessa de que "vou defini-la depois", enquanto `defffi` declara que "outra pessoa já escreveu e compilou o
corpo".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

O nome no typelisp e o nome do símbolo em C podem ser escritos separadamente porque os identificadores do
typelisp costumam conter `-` e os do C não podem. Se o nome C for omitido, o nome é usado como está como nome do
símbolo em C.

**As chamadas exigem `(unsafe ...)`** (mesmo para funções que só usam escalares). O compilador não tem como
confirmar que a assinatura C declarada bate com a real e só pode confiar na declaração; `unsafe` é a marca de que
você assume essa responsabilidade. O jeito pretendido é envolvê-la uma vez e criar um invólucro seguro:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; daqui em diante não é preciso unsafe
```

Os tipos que podem ser escritos são `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void) `string`
`ptr` `c-long` `c-ulong`, e os ponteiros tipados `(ptr T)` ([abaixo](#def-c-struct-e-ponteiros-tipados--alocar-structs-c)).

`string` é `const char *`. As strings do typelisp não terminam em NUL e podem elas mesmas conter NUL, então **são
copiadas para uma string C ao serem passadas**, e liberadas depois da chamada. Um NUL na string é um erro: o C
só olharia até ele, então uma string diferente seria passada silenciosamente.

**As strings devolvidas também são copiadas**, e não são liberadas: o que o C devolve pertence ao C, e pode
apontar para uma tabela estática, como com `getenv`. As funções que devolvem memória que quem chama deve liberar
(`strdup` etc.) devem ser recebidas como `ptr` e liberadas por você.

As funções cujo resultado aponta para dentro de um argumento (`strchr`, `strstr`) também funcionam
corretamente: o resultado é copiado antes de o argumento ser liberado.

Se uma função declarada para devolver `string` devolver NULL, é um erro, porque `string` não tem valor que
signifique "não havia nada". Se NULL for possível, receba o resultado como `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Com `:library`, essa biblioteca compartilhada é aberta e o símbolo é procurado nela. Sem ela, o símbolo é
procurado no **próprio processo** (tudo o que já está ligado, incluindo a libc). Um nome curto como `sqlite3` é
procurado como `libsqlite3.dylib` / `libsqlite3.so` nessa ordem, e um nome contendo `/` é tratado como caminho.
As bibliotecas abertas nunca são fechadas: o código que aponta para suas funções continua em execução, então o
único tempo de vida correto é o do processo.

#### ptr / c-long / c-ulong — palavras de máquina brutas

`ptr` é um ponteiro opaco (`void *`, `FILE *`, o que quer que a declaração quisesse dizer). `c-long` / `c-ulong`
são o `long` / `unsigned long` do C (também `size_t`, `int64_t` e `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Não chamá-los de `i64` / `u64` é proposital.** Esta linguagem não tem tipo inteiro de 64 bits, porque um
imediato com etiqueta tem só 63 bits ([capítulo 2](#2-escrita-de-tipos)). O nome `c-long` diz "isto é uma
palavra que atravessa a fronteira com o C, não um inteiro desta linguagem".

**Eles não têm aritmética.** `(+ x 1)` não pode ser escrito. Poderia ser oferecida, mas não é, para que nenhum
cálculo seja feito sobre um valor que não pode ser guardado em lugar nenhum e tem uma largura diferente da de
todos os outros números, pelo mesmo motivo pelo qual o tipo inteiro de 64 bits ficou de fora. Só há
**conversões**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; ler o que voltou
(as int (unsafe (c-strlen s)))               ; esta para lê-lo exatamente (int não perde 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; perguntar se cabe
(as c-ulong n)                               ; criar um a partir de outro inteiro
```

Os **literais** inteiros assumem o tipo esperado, então não é preciso `as` só para passar um:

```lisp
(unsafe (c-malloc 16))                       ; 16 é lido como um c-ulong
```

Os literais fora do intervalo são rejeitados como nas outras larguras (`(c-malloc -1)` não cabe em um `c-ulong`).

**Os lugares onde podem aparecer são limitados**: só tipos de argumento, tipos de retorno e variáveis locais.
Cada um dos seguintes é um erro:

```lisp
(defstruct handle (p ptr))          ; um campo de estrutura
(defenum maybe (none) (some ptr))   ; um campo de enumeração
(defvar (block ptr) ...)            ; uma global
(defffi f ((vector ptr)) i32)       ; dentro de um argumento de tipo
```

Há um único motivo para todos: **o slot põe uma etiqueta no que contém**. A etiqueta faria perder os bits mais
altos do ponteiro, o mesmo motivo pelo qual o tipo inteiro de 64 bits ficou de fora, então não é permitido nem
mesmo em `unsafe`. Não é uma questão de permissão: essa representação não existe.

Pelo mesmo motivo, eles não podem ser variáveis locais **capturadas** por funções aninhadas (um vínculo
capturado vai para uma célula, e uma célula põe uma etiqueta no que contém). Isso é sabido em tempo de compilação
e é relatado por `(compile f)`.

O GC não rastreia `ptr`. Ele aponta para fora do heap, então é o correto.

Quatro coisas não podem ser declaradas:

- **Argumentos variádicos** (`printf`). A parte variádica é passada com regras diferentes das dos argumentos fixos
  (na pilha no AArch64 Darwin), então não pode ser chamada corretamente a partir de uma assinatura fixa. `&rest`
  é rejeitado.
- **Passar ou devolver structs por valor.** Pelo mesmo motivo (depende da convenção de chamada de cada
  plataforma). Os tipos que podem ser escritos se limitam à lista acima, então isso não pode ser escrito.
- **Genéricos.** O C não tem equivalente.
- **O mesmo nome de uma função embutida.** Uma chamada compilada resolveria esse nome para a embutida, então é
  recusado em vez de dar errado silenciosamente.

#### Callbacks — fazer o C chamar de volta

Escrever um tipo de função `(fn (tipos...) tipo-de-retorno)` como tipo de um argumento faz desse argumento uma
função que o C chama de volta (um callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; uma função de nível superior
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; uma lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; uma função local
```

Um ponteiro de função C não passa de um endereço de código, e o C o chama passando só os argumentos declarados.
Não há lugar para passar variáveis capturadas, então **só podem ser passadas funções sem variáveis livres**, e
isso é verificado na verificação de tipos.

- Escreva um nome de função ou uma expressão `lambda` **diretamente** como argumento real. Uma variável contendo
  uma função não pode ser passada: qual função ela contém, e portanto se tem variáveis livres, só é sabido em
  tempo de execução.
- Uma `lambda` é um erro se ela se referir a variáveis locais de fora dela. Pode se referir a variáveis globais e
  a funções de nível superior.
- Uma função local (`labels`) não pode ter variáveis livres, incluindo as das funções irmãs que ela chama. As
  funções irmãs compartilham o lugar onde as variáveis capturadas são guardadas, então o que uma irmã chamada
  captura também é capturado por esta função.
- Uma função genérica obtém seus tipos do tipo de função declarado.
- Os tipos que podem ser escritos no tipo de função são os mesmos da lista acima. Porém, `string` não pode ser o
  tipo de retorno de um callback (entregaria ao C memória que ninguém libera). Um argumento `string` copia a
  string que o C passou para uma string do typelisp.

As chamadas a funções C só podem ser escritas dentro de `unsafe`, então os callbacks só podem ser passados dentro
de `unsafe`.

**O callback só pode ser chamado enquanto a função C que o typelisp chamou está em execução.** Se ele for chamado
de qualquer outro lugar (uma thread que não executa typelisp, um tratador de sinal, uma função registrada com
`atexit`), imprime o motivo e para o processo.

**As falhas não se propagam através do C.** Um `panic` ou `throw` dentro do callback não pode desenrolar através
dos quadros do C (seria comportamento indefinido), então 0 é devolvido ao C, e a falha é relançada para quem
chamou quando a função C retorna. Se o callback for chamado de novo entre a falha e o retorno da função C, ele não
é executado e 0 é devolvido.

Uma operação que precisaria esperar dentro de um callback (um `recv` em um canal vazio etc.) é um erro
([12.6](#126-código-compilado-e-tarefas)).

Quando uma função é redefinida, a nova definição é chamada a partir da próxima vez que ela for passada ao C.

Funciona do mesmo jeito com AOT (`compile-file`). Os pontos de entrada que o C chama são embutidos no executável.

**Elas não podem ser passadas como valores.** Uma declaração FFI não pode ser escrita como está para o `f` de
`(map f xs)`: um valor de função é uma closure que envolve o corpo de uma definição, e esta declaração não tem
corpo para envolver. Envolva-a em uma `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` também é recusado: o que poderia ser mostrado é o código de máquina do C, que este
compilador não produziu. `(compile c-abs)` dá certo (e não faz nada, já que ela já está compilada).

**Também funciona com AOT (`compile-file`).** O ligador resolve as próprias funções C. Se uma declaração tiver
`:library`, essa biblioteca é acrescentada à linha de ligação como `-l` (duplicatas são reunidas em uma), então
`compile-file` não precisa de argumentos extras. O próprio `compile-file` lê o código-fonte, então consegue
coletá-las das declarações.

Os símbolos também são procurados na build. Se uma função declarada não existir, o erro a nomeia antes de
qualquer erro de ligação.

A biblioteca padrão (o prelude) não usa `defffi`. A biblioteca padrão entra inteira em todo executável, então
uma declaração com `:library` nela ligaria essa biblioteca até em programas que não usam o FFI.

#### def-c-struct e ponteiros tipados — alocar structs C

```lisp
(unsafe
  (def-c-struct nome (campo tipo)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Declara uma struct com o mesmo layout que em C. Só pode ser escrita dentro de um `unsafe` de nível superior (que
não pode conter nada além de `def-c-struct`). Uma docstring pode ser posta logo depois do nome.

Os tipos que podem ser escritos para os campos são `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, os ponteiros tipados `(ptr T)` e outras `def-c-struct` (embutidas por valor). O layout (o
deslocamento de cada campo, e o tamanho e o alinhamento da struct) é calculado pelas regras do C (supondo LP64).
Um campo que aponta para a própria struct pode ser escrito, mas a struct não pode embutir a si mesma.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x em 0, y em 8, tamanho 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

O nome de uma `def-c-struct` entra no namespace de tipos (não pode haver um `defstruct` ou afim com o mesmo nome
no mesmo módulo), mas **não é o tipo de um valor**. Você não pode escrever `(defun f ((p point)) ...)`; ele só
aparece como aquilo para que um ponteiro tipado aponta.

**Um ponteiro tipado `(ptr T)`** é um endereço que aponta para um `T`. `T` é um dos tipos que podem ser escritos
para os campos acima. É uma palavra de máquina bruta como `ptr`, com as mesmas regras sobre onde pode aparecer
(só argumentos, tipos de retorno e variáveis locais; só pode ser um valor dentro de `unsafe`).

A alocação, a leitura e a escrita são escritas nas formas a seguir. Todas só podem ser usadas dentro de
`unsafe`.

| Forma | Significado |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Aloca `n` valores de `T` (1 se omitido). O conteúdo é preenchido com 0. Devolve um `(ptr T)` |
| `(c-ref p i)` | Um ponteiro para o elemento `i` a partir de `p`. Erro se estiver fora do intervalo alocado |
| `(c-deref p)` / `(setf (c-deref p) v)` | Lê / escreve o escalar para o qual `p` aponta |
| `p::field` / `(setf p::field v)` | Lê / escreve um campo de uma struct. Ler um campo que é uma struct embutida dá o seu endereço (`(ptr tipo-interno)`) |
| `(as ptr p)` | Esquece o tipo, tornando-o um `ptr` (para passá-lo a algo como o `void *` de `qsort`). Não há conversão de volta |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**A memória alocada é liberada quando o controle sai do `unsafe` que a alocou.** O dono é o `unsafe`
lexicamente mais externo dentro da mesma função. Ela é liberada tanto se o código terminar normalmente quanto se
sair por `panic`, `throw` ou `return-from`. As funções de `lambda` e `labels` são funções separadas, então um
`c-alloc` nelas precisa de um `unsafe` próprio dentro delas.

Por isso, um ponteiro tipado não pode sair do `unsafe` que o alocou. Cada um dos seguintes é um erro de tipo:

- Torná-lo o valor da expressão `unsafe` (então também não pode ser devolvido de uma função)
- Capturá-lo em uma closure (`lambda`, `labels`)
- Passá-lo a `task` / `thread`
- Lançá-lo com `throw`

Para usar valores fora do `unsafe`, copie-os para um `defstruct` ou para números dentro do `unsafe` e devolva-os.

**A memória alocada do lado do C não é tratada.** Os valores que chegam do C como ponteiros tipados (valores de
retorno de `defffi`, argumentos de callbacks, valores lidos de campos de tipo ponteiro) são verificados em tempo
de execução para ver se apontam para um valor daquele tipo dentro de uma alocação viva de `c-alloc`, e são um erro
se não apontarem. NULL também é um erro. Para receber memória que o C alocou, ou NULL, use o `ptr` sem tipo (cujo
conteúdo não pode ser lido).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Quando o argumento de um callback é recusado pela verificação, isso é relatado a quem chamou quando a função C
retorna, assim como uma falha dentro de um callback.

### 3.4 defvar / defparameter / defconstant — variáveis globais

```lisp
(defvar (name Type) init-expr)        ; inicializa só se ainda não estiver vinculada
(defparameter (name Type) init-expr)  ; atribui toda vez
(defconstant (name Type) init-expr)

; com docstring (na mesma ordem de defvar/defparameter/defconstant do CL: depois do valor)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**A diferença entre `defvar` e `defparameter` aparece ao recarregar** (como no CL). Se a global **já estiver
vinculada, `defvar` nem sequer avalia o inicializador**, então quando você edita um arquivo de configuração e o
lê de novo, os valores que a sessão mudou ficam como estão. `defparameter` atribui toda vez, então lê-lo de novo
traz os valores de volta ao que está escrito.

A anotação de tipo é obrigatória (não é inferida do inicializador). `defvar` pode ser mudada; `defconstant` não
(`setf` é um erro).

### 3.5 defmethod — definição de métodos

```lisp
; método de instância: pode ser chamado como (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; função estática / associada: pode ser chamada como (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Quem chama resolve o método a partir do tipo estático de `obj` (despacho único e estático). Uma docstring pode
ser colocada na mesma posição e com as mesmas regras de `defun` (logo depois da cláusula `where`, no início do
corpo, só quando formas do corpo a seguem). O mesmo vale para os métodos dentro de `impl`; são recuperadas com
`(documentation Type::method)`.

Os parâmetros de tipo próprios de um método são escritos no nome dele com `<...>`, como em `defun`.
Os parâmetros de tipo do tipo receptor (`T` abaixo) são fixados pelo receptor; os do próprio método
(`U`) são inferidos a partir dos argumentos de cada chamada.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Os parâmetros de tipo próprios do método precisam de nomes diferentes dos parâmetros de tipo que o
  tipo receptor declara (o `T` de `(defstruct Box<T> ...)`) e dos nomes escritos no receptor.
- Se o tipo receptor for genérico, escreva no receptor todos os parâmetros de tipo dele como
  variáveis (`Box<T>`) ou todos como tipos concretos (`Box<int>`).
- Um método dentro de `impl` não pode acrescentar parâmetros de tipo: a assinatura dele segue a que
  o trait declara.

### 3.6 defstruct — estruturas (tipos definidos pelo usuário)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; genérica (parâmetros de tipo entre sinais de menor e maior)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Cada campo é `(name type)` ou `(pub name type)` (visibilidade por campo, independente do `pub` da própria
  estrutura). Uma expressão a mais no final vira o **valor padrão** do slot (`(x i32 0)`); veja a lista de opções
  abaixo.
- O seguinte é gerado automaticamente:
  - O construtor `Name::new` (argumentos na ordem dos campos)
  - Os leitores `(field-name instance)`, com a escrita `instance::field-name`
  - Os escritores `(set-field-name instance value)`, com a escrita `(setf instance::field-name value)`
- Para tornar a própria estrutura `pub`, ponha `pub` na frente, como em `(pub defstruct ...)`.
- **Defina um tipo antes de nomeá-lo.** O tipo de um campo pode ser a própria estrutura (`(next Option<node>)`),
  mas não um tipo definido depois: os tipos não têm uma declaração antecipada correspondente a `defsignature`.
  Um nome ainda não definido dá o mesmo erro `unknown type` em um tipo de argumento de `defun` ou em `the`. Então
  dois tipos que se referem um ao outro não podem ser escritos.
- **As variáveis de tipo são só as escritas em posições de declaração.** Para
  `defun`/`defstruct`/`defenum`/`deftype`, o `<T>` do nome; para `defmethod`, o tipo do receptor
  (`(self box<T>)`, ou `box<T>` para um método estático) e o `<U>` do nome do método; para `impl`, o
  tipo alvo e `impl<T>`; para `deftrait`, `Self` e os tipos associados de `(type Item)`. Um nome que
  aparece pela primeira vez em qualquer outro lugar (argumentos, o valor de retorno, `the`/`lambda`
  no corpo) não vira variável de tipo; é `unknown type`.
- **Docstrings**: um literal de string logo depois do nome, antes dos campos, vira a docstring
  (`(defstruct Name "doc" (field Type)...)`, a mesma posição do `defstruct` do CL). Um campo sempre tem a forma
  `(name Type ...)` e nunca pode ser uma string solta, então não há ambiguidade. Recupere-a com
  `(documentation Name)`.

#### Lista de opções

Escrever uma lista `(Name opção...)` na posição do nome especifica opções (a mesma posição do CL).

```lisp
(defstruct (point (:constructor make-point)          ; construtor com palavras-chave
                  (:constructor at (x &optional y))  ; construtor BOA
                  (:copier copy-point))
  (x i32 0)          ; um terceiro elemento é o valor padrão daquele slot
  (y i32 0))

(point::make-point :y 7)   ; x é 0
(point::at 1)              ; y é 0
(point::at 1 2)
(copy-point p)             ; uma cópia rasa (o mesmo que o copier do CL)
```

- **`:constructor`**: o que é gerado é uma **função estática** do tipo (`point::make-point`), cujo corpo é sempre
  `(point::new ...)`. `new` continua sendo o único construtor estrutural; o que é criado aqui é uma *forma de
  chamá-lo*. Podem ser declarados vários.
  - `(:constructor name)` recebe todos os slots como `&key`. **Todo slot precisa de padrão** (esta linguagem não
    tem nada que corresponda ao "slot não vinculado" do CL).
  - `(:constructor name (slot...))` recebe os slots nomeados como argumentos posicionais (em qualquer ordem). Os
    slots não nomeados são preenchidos com seus padrões, então **precisam de padrões**. Depois de `&optional`, o
    resto pode ser omitido (e da mesma forma precisa de padrões).
- **`:copier`**: gera um **método de instância** que devolve um valor novo com os mesmos valores de slot. Raso,
  como o copier do CL.
- **`:include Parent`**: põe os slots do pai na frente (os padrões também são herdados; o pai pode estar em outro
  arquivo). **Não cria relação de tipos**: o filho não é subtipo do pai, os métodos do pai não se aplicam ao
  filho, e não há verificação em tempo de execução ligando os dois. Esta linguagem não tem subtipagem; as
  interfaces em comum são trabalho de `deftrait`. Só a *lista* de slots é unida.
- **Os padrões dos slots só são lidos pelos construtores gerados.** Escrever um padrão sem declarar nenhum
  `:constructor` é um erro, já que ele nunca poderia ser usado.
- Opções deixadas de fora, e por quê:
  - **`:conc-name`**: no CL ela põe um prefixo nos acessores para evitar colisões em um único namespace de
    funções plano. Aqui, os acessores são métodos despachados pelo tipo do receptor, então não há colisões, e um
    prefixo quebraria `instance::field` (que só conhece o nome do slot).
  - **`:predicate`**: responde em tempo de execução "este valor é um `point`?". Aqui os tipos são uma
    classificação em tempo de compilação sem testemunha em tempo de execução, e não há posição onde exista "um
    valor de tipo desconhecido que poderia ser um point" (`match` sobre `Sexpr` é fechado, e `:dyn` não admite
    downcast), então um predicado gerado só poderia devolver sempre `true`.
  - **`:type` / `:initial-offset` / `:named`**: substituem a representação do valor por uma lista ou um vetor. A
    representação pertence ao compilador e não pode ser observada a partir da linguagem.

### 3.7 defenum — enumerações (tipos soma)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; uma variante com carga (campos posicionais)
  (Variant2)                  ; uma variante sem carga
  ...)

; genérica
(defenum Option<T>
  (Some T)
  (None))
```

- Cada variante tem a forma `(VariantName FieldType...)`. Os campos são só posicionais (não têm nome). É preciso
  pelo menos uma variante, e os nomes não podem se repetir.
- Os valores são construídos, como com os `Option`/`Result` embutidos, qualificados ou por meio de `use`:
  `(Name::Variant1 a b)`, ou `(Variant1 a b)` depois de `(use Name)`.
- Podem ser desmontados com `match` / `if-let`. `match` verifica a exaustividade (precisa cobrir todas as
  variantes ou ter um `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Os métodos e as funções associadas são acrescentados depois com `defmethod`/`impl`, como com `defstruct`.
- Para tornar a própria enumeração `pub`, escreva `(pub defenum ...)`.
- **Docstrings**: a mesma posição e as mesmas regras de `defstruct`, logo depois do nome, antes das variantes
  (`(defenum Name "doc" (Variant ...)...)`). Recupere-a com `(documentation Name)`.

### 3.8 deftype — apelidos de tipo

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

O `deftype` do CL, reduzido ao que faz sentido em uma linguagem de tipagem estática: **uma forma de escrever um
tipo, não um tipo**.

- A posição do nome é a mesma de `defun`, e os argumentos genéricos são escritos `Name<T,U>`. No lugar de uso, é
  preciso exatamente o número declarado de argumentos de tipo (demais ou de menos é um erro na hora).
- A expansão acontece **dentro do analisador de tipos**. Então nada mais adiante sabe que o apelido existe: as
  chaves de monomorfização, os dumps, o caminho de compilação e **as mensagens de erro** mostram todos a forma
  expandida. Se `(f "x")` falhar contra uma função que exige `meters`, a mensagem diz `i32`.
- **Não é um tipo novo.** `(deftype meters i32)` faz de `meters` e `i32` o mesmo tipo, então misturá-los não é
  detectado. Se quiser mantê-los separados, use `defstruct`.
- **Não é um predicado.** O `(deftype small () '(integer 0 9))` do CL descreve um *conjunto de valores* que
  `typep` testa em tempo de execução, mas aqui os tipos são uma classificação em tempo de compilação sem
  testemunha em tempo de execução, então um apelido que restringisse valores não teria nada a restringir.
- **Não pode conter a si mesmo.** Um apelido é expandido onde é escrito, então não tem para onde recursar. Os
  tipos de dados recursivos são escritos com `defstruct`/`defenum`.
- Compartilha o namespace com os tipos e os traits (dentro de um módulo não pode ter o mesmo nome de um
  `defstruct`/`defenum`/`deftrait`). Torne-o público com `(pub deftype ...)` e traga-o com `(use m::meters)`.
- **Docstrings**: logo depois do nome, antes do tipo (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; a lista de supertraits é obrigatória; () se não houver nenhum
  (type AssocName)                       ; tipos associados (qualquer número, opcionais)
  (method-name ((self Self) params...) RetType)          ; sem corpo = precisa ser implementado
  (method-name ((self Self) params...) RetType body...)) ; com corpo = implementação padrão

(impl TraitName TargetType
  (where (Trait A)...)                   ; restrições que valem para o impl inteiro (opcional)
  (type AssocName ConcreteType)          ; torna concreto um tipo associado
  (method-name (recv params...) RetType body...))
```

Por meio de `impl`, cada método é registrado como um `defmethod` comum de `TargetType`. Os traits são referidos
como restrições de trait nas cláusulas `where` das funções genéricas (veja
[3.1 defun](#31-defun--definição-de-funções)). Um nome de trait também pode ser um caminho `::` como `m::Trait`.

**A lista de supertraits (obrigatória)**: sempre escrita logo depois do nome do trait. Cada elemento é um nome de
trait simples ou, se esse trait tiver tipos associados, `(Trait (Assoc Type))` com **todos os seus tipos
associados fixados**.

```lisp
(deftrait Eq () ...)                       ; sem supertraits
(deftrait Ord (Eq) ...)                    ; o trait Ord: Eq do Rust
(deftrait CharSource ((Iter (Item char)))  ; fixar um tipo associado
  (rewind ((self Self)) ()))
```

A herança tem três efeitos. (1) `impl Ord X` exige que `impl Eq X` seja escrito **antes** (uma regra sobre a
ordem de escrita: a única forma que pode ser decidida de modo determinístico no REPL e com `load` passo a passo,
e mais estrita que o Rust). (2) `(where (Ord T))` sozinho permite chamar também os métodos de `Eq`. (3) Os métodos
de `Eq` podem ser chamados por meio de um `:dyn Ord`, e um valor `:dyn Ord` pode ser passado como está onde se
exige um `:dyn Eq` (upcast). Um subtrait redeclarar um método com o mesmo nome do pai, e herdar métodos com o
mesmo nome de dois pais, são ambos erros (uma vtable tem um slot por nome). A herança em diamante se funde em um
único slot.

**Implementações padrão**: um corpo depois da assinatura é usado quando um `impl` omite o método. O corpo é
resolvido no **namespace do módulo** em que o trait está escrito, então pode chamar funções não públicas desse
módulo. Os métodos com corpo também podem ter cláusulas `where` e docstrings. O corpo tem o tipo verificado
**uma vez, no ponto de declaração**, com `Self` deixado como variável de tipo (restrita por
`Self: o próprio trait`), como no Rust: os erros que falhariam para todo `impl` e todo tipo que o implemente,
mesmo em padrões que nenhum `impl` jamais omite, são pegos ali. As chamadas sobre `self` a métodos do próprio
trait ou de seus supertraits passam por essa restrição, e os tipos associados são fixados a si mesmos, então uma
assinatura que devolve `Item` é confrontada com o corpo sem conhecer o tipo concreto.

**Implementações gerais (blanket)**: tornar o alvo uma variável de tipo implementa o trait de uma vez para todo
tipo que atenda às restrições.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; sem corpo nenhum; tudo é a implementação padrão
```

**Nenhum código é gerado até que um tipo concreto realmente o use** (uma vez por tipo, pelo mesmo mecanismo da
monomorfização comum). Um trait pode ter no máximo uma implementação geral. Se um tipo tiver um `impl` explícito,
esse tem prioridade. A verificação de tipos do corpo é separada da geração: é feita uma vez no ponto de
declaração, **com o alvo deixado como variável de tipo** (como no Rust), então até uma implementação que nunca é
usada tem seus erros pegos ali se eles falhassem para todo alvo sob as restrições declaradas. As chamadas
justificadas pelas restrições (`(less self other)` sob `(where (Ord T))` etc.) passam, como no corpo de um
`defun` genérico.

**Docstrings**: um `deftrait` pode ter uma docstring para o trait inteiro, como literal de string logo depois da
lista de supertraits, antes dos itens (`(deftrait Name () "doc" (type ...) (method ...)...)`). Uma assinatura sem
corpo não pode ter docstring: uma string final seria ela mesma o valor de retorno de uma implementação padrão,
então as duas não poderiam ser distinguidas.

Os traits que a biblioteca padrão oferece: **`Iter`** (`next` / tipo associado `Item`; a base de `doiter` e das
funções de sequência), **`Eq`** (`equals`; `not-equals` é uma implementação padrão), **`Ord`** (herda `Eq`; só
`less` precisa ser implementado, e `less-equal` / `greater` / `greater-equal` são implementações padrão),
**`Error`** (`message` / `source`; `:dyn Error` para tratar os tipos de erro de modo uniforme),
**`print-object`** (uma representação impressa por tipo), **`Pathish`** (designadores de nome de caminho: uma
string ou um `pathname`), e a hierarquia de streams **`Stream`** → **`InputStream`** / **`OutputStream`** →
**`CharInput`** / **`CharOutput`** → **`PeekInput`**. Quais tipos implementam quais traits está em
[types.md](types.md); os métodos de cada trait, em [Traits padrão](functions/traits.md),
[Tipos de erro](functions/option-result.md#3-tipos-de-erro-e-o-trait-error),
[print-object](functions/printing.md#5-print-object-representação-impressa-por-tipo) e
[Streams](functions/streams-files.md). Se você fizer `impl` de `Iter` para seu próprio tipo de coleção, `doiter`
(capítulo 5) e `map` / `filter` / `sort` e afins funcionam sobre ele como estão.

As chamadas de trait são **estáticas** por padrão (resolvidas pelo tipo estático do receptor). Para tratar valores
cujo tipo concreto é decidido em tempo de execução, o tipo de objeto trait `:dyn Trait` (capítulo 2) dá despacho
dinâmico por meio de uma vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; um ponto de chamada, uma resposta por implementação
```

Só os traits em que "todo método tem um receptor `self`, não usa `Self` em nenhum lugar além do receptor, e não é
ele mesmo nem genérico nem variádico" podem virar `:dyn` (os métodos herdados precisam cumprir as mesmas
condições).

Só os tipos cujos valores têm representação no heap podem ir em uma caixa `:dyn`:

| Podem ir | Não podem ir |
|---|---|
| Os tipos `defstruct` / `defenum` (incluindo `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` e as estruturas da biblioteca padrão), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Os inteiros de largura fixa (`i8` a `u32`), `f32`, `bool`, `char`, `symbol`, `()`, os tipos de função e `Option<T>` sem caixa ([a representação em tempo de execução de Option](functions/option-result.md#2-a-representação-em-tempo-de-execução-de-optiont)) |

Colocar um valor de um tipo que não pode ir onde se espera um `:dyn` é um erro de tipo. Para tratar esses valores
por meio de `:dyn`, envolva-os em uma estrutura, como em `(defstruct flag (v bool))`.

### 3.10 module / use — namespaces

```lisp
(module path body...)      ; path é uma sequência de segmentos como foo ou foo::bar
(in-module path)           ; daqui até o fim desta unidade, dentro de path (a forma plana de module)
(use path...)              ; cria apelidos de funções, tipos e módulos no namespace atual
(import path...)           ; o mesmo que use (uma escrita compatível com o CL)
(shadowing-import path...) ; um use que toma de propósito um nome simples já em uso
```

- `module` cria um namespace. **Os tipos não são namespaces** (como no Rust, um tipo só tem funções associadas
  e métodos).
- Fazer `use` de um tipo torna seus construtores e seus métodos estáticos públicos também disponíveis pelo nome
  simples (por exemplo, depois de `(use option)`, `some`/`none` podem ser chamados sem
  `option::some`/`option::none`).
- A ordem de resolução dos nomes simples (identificadores não qualificados): formas especiais → construtores →
  funções livres (namespace atual → raiz) → métodos de instância (resolvidos pelo tipo estático do primeiro
  argumento). Não sobe pelos módulos pai intermediários.
- Um caminho qualificado `a::b` resolve `a` na ordem acima; se for um módulo, entra nele, e se for um tipo, o
  último segmento é resolvido como um item associado.
- **`use` afeta as formas que vêm depois dele.** Um arquivo é lido uma forma por vez, e as dependências são
  resolvidas logo antes de a forma ser verificada, então escrever `m::f` **acima** de `(use m)` dá
  `unresolved path`. Ponha o `use` no início do arquivo.
- **`use` pode receber vários caminhos** (`(use a::f b::g)`). `import` é uma escrita compatível com o CL com o
  mesmo comportamento.
- **Um `use` cujo nome simples já está em uso é relatado.** Ao resolver um nome simples, olham-se as próprias
  definições do módulo antes dos apelidos, então `(use m::twice)` depois de `(defun twice ...)` **não faz nada**.
  Se é isso que você quer, escreva `shadowing-import` (mesmo assim ele não consegue vencer uma definição, já que
  não há como removê-la; só vence apelidos anteriores).
- **`in-module` é a forma plana de `(module path body...)`.** Escrever `(in-module geometry)` põe tudo dali até o
  fim da unidade (o arquivo, ou o corpo do `module` envolvente) dentro de `geometry`. Ele vai **dentro** do
  módulo do próprio arquivo (`main::geometry` para `main.typl`). Dois seguidos se aninham em ordem. É diferente do
  `in-package` do CL, e tem um nome diferente: neste sistema o arquivo já é um módulo, então não há nada a
  "selecionar", e tudo o que uma forma pode fazer é aninhar.

### 3.11 Arquivos e módulos (projetos com vários arquivos)

O caminho do arquivo relativo à raiz dos fontes é o caminho do módulo: o conteúdo de `<root>/geo/point.typl` é
envolvido implicitamente no módulo `geo::point` (um diretório também é um segmento, no estilo de Rust / Python).
Um `(module bar ...)` explícito no arquivo se aninha **dentro** dele (`geo::point::bar`), então o caminho derivado
e uma declaração explícita nunca colidem.

- **Raiz dos fontes**: coloque um arquivo de manifesto `typelisp.toml` na raiz do projeto (pode estar vazio;
  opcionalmente uma linha `src = "src"` indica o diretório dos fontes). Ele é encontrado subindo a partir do
  diretório do arquivo alvo. Sem manifesto, o diretório do arquivo de entrada (o diretório atual para o REPL) é
  a raiz.
- **Carregamento sob demanda**: quando `(use geo::point)` se refere a um módulo ainda não carregado, o arquivo
  correspondente (`geo/point.typl`) é carregado, tem o tipo verificado e é registrado automaticamente.
  `use a::b::c` procura primeiro o prefixo mais longo: `a/b/c.typl` → `a/b.typl` → `a.typl` (já que `c` pode ser
  um item dentro de um módulo). As definições visíveis de outros módulos precisam de `pub`
  ([3.13 pub](#313-pub--visibilidade)).
- **As referências circulares são erros**: a cadeia é relatada na forma
  `circular module dependency: a -> b -> a`.
- **Execução**: `typl <file.typl>` executa um arquivo (sem argumentos, o REPL). O `use` no REPL resolve os
  arquivos pelas mesmas regras.
- **Capacidade da arena de cons**: `typl --heap-cells N` define a **capacidade inicial** da arena de células cons
  (65536 por padrão; a forma `--heap-cells=N` também funciona, tanto para executar arquivos quanto no REPL). A
  arena **cresce acrescentando mais** quando fica curta. O limite de crescimento é 256 vezes a capacidade inicial,
  e uma alocação além disso dá `heap exhausted`: a capacidade inicial significa "alocar isto no começo", e o
  limite significa "além daqui, tratar como vazamento".

### 3.12 load — carregamento plano

```lisp
(load "path")   ; só no nível superior; path é um literal de string
```

- **Carregamento plano** no estilo do CL: lê as formas do arquivo alvo **no namespace atual** como estão (sem
  envolvê-las em um módulo, ao contrário de `use`). Só no nível superior (dentro do corpo de uma função é um erro
  de tipo).
- `path` é relativo ao diretório do arquivo que carrega (a partir do REPL, ao diretório de trabalho do
  processo). Se não tiver extensão, `.typl` é acrescentado.
- Os `(load ...)`/`(use ...)` do arquivo carregado também são processados recursivamente.
- **Lê uma forma por vez e a executa na hora** (como faz o `load` do CL). A forma *k* terminou de ser executada
  antes de *k+1* ser lida: mesmo que haja um erro de sintaxe ou de tipo no meio, as formas anteriores já foram
  executadas. Os arquivos de módulo carregados por `use` são diferentes: são verificados como uma unidade e sua
  execução fica a cargo de quem os usou com `use` (corresponde ao `compile-file` do CL).

### 3.13 pub — visibilidade

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` só pode ser posto nos onze tipos acima (não em `module`/`use`/`deftrait`/`impl`). Ele é escrito com a
palavra-chave da definição logo depois de `pub`, não na forma `(pub (defun ...))` que envolve a definição entre
parênteses. Um `pub` torna pública exatamente uma definição (não se podem marcar várias definições de uma vez).

### 3.14 defmacro — definição de macros

```lisp
(defmacro name (obrigatórios... &optional opt... &rest rest-name &key key...) body...)
```

- Todos os parâmetros e o valor de retorno são sempre `Sexpr`, então não se escrevem anotações de tipo.
- Macros não higiênicas no estilo do CL (evitar colisões com `gensym` é responsabilidade de quem escreve a
  macro).
- A lista lambda segue a ordem do CL `obrigatórios &optional &rest &key` (cada marcador no máximo uma vez, e só
  nesta ordem).
  - `&optional` … argumentos opcionais. `name` ou `(name expr-padrão)`. A expressão padrão é avaliada na expansão
    (pode se referir a parâmetros vinculados antes) e vinculada quando o argumento é omitido (sem padrão, a lista
    vazia `()`).
  - `&rest name` … recebe os argumentos posicionais restantes juntos como uma lista `Sexpr`.
  - `&key` … argumentos de palavra-chave. `name` ou `(name expr-padrão)`. Quem chama os passa como `:name valor`
    (em qualquer ordem). Quando omitidos, a expressão padrão (a lista vazia `()` se não houver). Palavras-chave
    desconhecidas ou uma sequência `:key` de comprimento ímpar são erros.
- Exemplos: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — vínculos de macros locais

```lisp
(macrolet ((name (lista-lambda) body...) ...) body...)   ; macros de escopo léxico
(symbol-macrolet ((name expansão) ...) body...)           ; um nome representa uma forma
```

Ambas são formas especiais de **expressão**, e nada resta em tempo de execução (o que é compilado é a forma
expandida do corpo). A lista lambda é a mesma de `defmacro`. As regras detalhadas e os exemplos estão em
[Vínculos de macros locais](functions/system.md#9-vínculos-de-macros-locais-macrolet--symbol-macrolet).

## 4. Vínculos e condicionais

```lisp
(let ((name val) ...) body...)      ; vínculo em paralelo
(let* ((name val) ...) body...)     ; vínculo sequencial (vínculos anteriores podem ser usados em inicializadores posteriores)

(if cond then else)                 ; else é obrigatório (sempre três elementos)
(when cond body...)                 ; um if sem else (tipo Unit). defmacro
(unless cond body...)               ; a negação de when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; uma lista de chaves: casa se alguma delas casar
  (else body...))                   ; expr é avaliada uma vez. as chaves são comparadas com equal.
                                     ; as chaves são "literais" e não são avaliadas (como no CL).
                                     ; um símbolo simples a significa o símbolo 'a.
                                     ; escrever 'a é um erro (use o a simples). defmacro
(ecase expr (key body...) ...)      ; um case que exige casamento. panic se nada casar. defmacro
(ccase expr (key body...) ...)      ; o ccase do CL. não há restarts a oferecer, então é igual ao ecase. defmacro
(and expr...)                       ; avaliação em curto-circuito. true com zero argumentos. defmacro
(or expr...)                        ; avaliação em curto-circuito. false com zero argumentos. defmacro
(progn body...)                     ; executa em ordem e devolve o último valor
(unsafe body...)                    ; o mesmo que progn, mais a permissão para escrever chamadas FFI
                                     ; e palavras brutas. veja 3.3 defffi
(prog1 form more...)                ; avalia tudo; o valor é o de form. defmacro
(prog2 a b more...)                 ; avalia tudo; o valor é o de b. defmacro
(the Type expr)                     ; uma anotação de tipo (sem efeito em tempo de execução)
```

### 4.1 unsafe — assumir o que não pode ser verificado

```lisp
(unsafe body...)
```

O mesmo que `progn`: avalia o corpo em ordem e devolve o último valor. Não cria escopo e não é uma fronteira de
função (`break` / `return-from` o atravessam direto para fora). A diferença é que algumas coisas só podem ser
escritas dentro dele.

Atualmente três coisas exigem `unsafe`: chamar funções C declaradas com
[defffi](#33-defffi--declarar-funções-c-ffi), tornar palavras de máquina brutas (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) valores, e [`def-c-struct` e `c-alloc`](#def-c-struct-e-ponteiros-tipados--alocar-structs-c).

A memória alocada com `c-alloc` é liberada ao sair do `unsafe` mais externo dentro da mesma função. Só esse
`unsafe`, ao contrário de `progn`, tem trabalho a fazer na saída: a liberação.

O que `unsafe` assume são as seguintes suposições que o compilador não consegue verificar:

- **Que os tipos batem.** Que a assinatura C declarada bate com a real. Se não bater, os argumentos vão para os
  registradores errados e os valores de retorno são lidos com a largura errada.
- **A segurança de memória.** O que o lado C faz com o que recebe.
- **O estado do processo inteiro.** Variáveis de ambiente, tratadores de sinal, `errno`. Por exemplo, chamar
  `setenv` pelo FFI quebra as suposições que o `decode-universal-time` desta implementação faz quando calcula o
  horário local.
- **A segurança entre threads.**

Não é uma saída da verificação de tipos. `(unsafe (+ 1 "two"))` não passa. O que é permitido é escrever certas
**operações**, não escrever disparates.

Funciona lexicamente. O corpo de uma `lambda` escrita dentro de `unsafe` herda a permissão (como com as closures
dentro dos blocos `unsafe` do Rust). O valor pode ser chamado mais tarde de fora do `unsafe`, mas escrevê-lo ali é
tomado em si como aceitar a responsabilidade.

### 4.2 destructuring-bind — desmontar listas pela forma

```lisp
(destructuring-bind lista-lambda form body...)
```

Desmonta **pela forma** a lista que `form` produz e a vincula. A lista lambda é a de `defmacro` (obrigatórios →
`&optional` → `&rest`/`&body` → `&key`, cada um com expressões padrão), pelo mesmo motivo pelo qual o CL
compartilha uma entre os dois: são duas formas que desmontam a mesma coisa.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Toda variável vinculada é um `Option<Sexpr>`.** Não é uma limitação da implementação, mas a natureza do que é
  vinculado: as listas de expressões S são as únicas listas desta linguagem, então não há outro tipo a dar aos
  elementos. Recorrer a `match` onde é preciso um escalar é igual a fazê-lo no corpo de um `defmacro`.
- **Uma forma que não casa causa panic** (corresponde ao erro do CL): elementos de menos ou de mais, uma
  sequência `&key` de comprimento ímpar, ou uma palavra-chave desconhecida. `sexpr-car` é uma função tolerante que
  devolve `()` para `()`, então sem a verificação uma lista curta seria vinculada silenciosamente a uma sequência
  vazia.
- **Listas lambda aninhadas não são suportadas.** `defmacro` também não as aceita, então há uma regra só.
  `(a (b c))` não vincula silenciosamente uma sublista a `b`; é um erro que diz isso.
- As expressões padrão de `&optional` / `&key` **só são avaliadas quando usadas** (como no CL).
- Não há nada que corresponda ao `&allow-other-keys` do CL (`defmacro` também não tem).

### 4.3 match — casamento de padrões

```lisp
(match expr
  (pattern body...)
  ...)
```

Tipos de padrão:
- `_` — curinga
- Um nome de variável — um padrão de vínculo (sempre casa). Porém, se o tipo do valor examinado tiver uma
  variante com esse nome, ele é resolvido como **o padrão de nome de variante simples abaixo**
- Um nome de variante simples — casa com uma variante que não recebe argumentos (`(match c (red 1) (blue 2))`).
  Escrever uma variante com campos pelo nome simples é um erro de aridade, então escreva-a entre parênteses, como
  em `(circle r)`
- **Literais imediatos**: inteiros / `true`/`false` / caracteres — comparados como palavras
- **Literais de valor**: strings / números de ponto flutuante / símbolos (`'foo`) / inteiros bignum / ratios —
  comparados por valor com o `Eq::equals` desse tipo ([Traits padrão](functions/traits.md#2-eq--ord-comparação)).
  As strings são comparadas pelo conteúdo, não pela identidade
- `(= expr)` — avalia qualquer expressão e compara com `Eq::equals`. A única forma de comparar tipos que não têm
  sintaxe literal (instâncias de `defstruct`, globais, resultados calculados), e uma implementação de `Eq`
  definida pelo usuário vira a regra de comparação como está. `expr` pode se referir a qualquer coisa visível da
  posição do ramo (argumentos, vínculos externos, globais)
- `(Ctor sub-pattern...)` — padrões de construtor (`Some x` `None` `Cons a d` `Ok v` etc.)

Comparar um tipo que não implementa `Eq` com um literal de valor / `(= expr)` é um erro de tipo (esta linguagem
prefere dizer "não podem ser comparados" a deixar um ramo que silenciosamente nunca casa).

**Literais de valor contra um valor examinado `Sexpr`**: o `Eq` de `sexpr` é `eq` (a identidade do CL), então os
imediatos (`'foo` (internado) / inteiros / caracteres / `true`/`false`) podem ser escritos como estão e casam pelo
conteúdo:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Os literais não imediatos (strings / números de ponto flutuante / inteiros bignum / ratios) **não podem ser
escritos** contra um `Sexpr`. O `eq` deles compara a identidade do objeto, o que daria "um ramo que passa na
verificação de tipos mas nunca casa", então é um erro que nomeia o padrão de variante: escreva `(str "hi")` e ele
é desmontado em uma `string` comparada pelo conteúdo. `(= expr)` pede explicitamente `equals`, então essa
restrição não se aplica a ele.

**O valor examinado não precisa ser um ADT.** `string`/`symbol`/`i32`/`f64` e afins podem passar por `match`
diretamente (é ali que entram os padrões de literais de string). Porém, um tipo sem variantes não pode ser coberto
por enumeração, então `_` (ou um padrão de vínculo agindo como curinga) é obrigatório:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; um tipo sem variantes precisa de `_`
```

Contra um valor examinado `Sexpr`, além dos 18 padrões de variante embutidos acima, podem ser escritos **padrões
de downcast** (para tirar instâncias de ADTs definidos pelo usuário): sintaxe para recuperar, com `match`, uma
instância de um `defstruct`/`defenum` (capítulo 3) que foi convertida implicitamente em `Sexpr`, como em
`(list p 42)`:

- `(TypeName sub-pattern...)` — decomposição por campos com o **nome do tipo** primeiro (só estruturas: um
  `defstruct` sempre tem uma variante, então é escrito com o nome do tipo em vez de um nome de variante). Por
  exemplo, para `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Um nome de variante simples `(VariantName sub-pattern...)` — extrai uma variante de um `defenum`. Resolvido como
  um nome simples visível depois de `(use EnumType)` (as mesmas regras de visibilidade de quando se chama o
  construtor). Por exemplo, para `(defenum color (red) (blue))`, `(red)` `(blue)` depois de `(use color)`. Se os
  nomes de variante de várias enumerações visíveis colidirem, é um erro de ambiguidade, então a forma qualificada
  `(EnumType::VariantName ...)` também pode ser escrita (sem precisar de `use`).
- `(the Type pattern)` — um downcast do tipo inteiro (vinculando-o inteiro). Não decompõe os campos; passa o
  valor a `pattern` como está. A única forma de tirar uma estrutura mutável mantendo sua identidade, e também a
  única forma de tirar um `Vector<T>`/`HashTable<K,V>` de um `Sexpr` (eles não têm forma de decomposição por
  campos). Por exemplo, depois de `(the point p)`, `(setf p::x 9)` também se reflete na instância original da
  lista.

**Padrões para `Option<Sexpr>`**: o tipo dos dados de expressões S não é `Sexpr`, mas `Option<Sexpr>`, e a lista
vazia não é uma variante de `Sexpr`, mas o `none` de `Option`. Então, ao fazer `match` de um `Option<Sexpr>`, as
18 variantes de `Sexpr` e `none` podem ser escritas **planas na mesma lista de ramos** (não é preciso um `match`
externo para tirar o `Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; a lista vazia
    (_          9)))
```

A exaustividade é verificada no mesmo universo plano: as 18 variantes de `Sexpr` mais `none`, 19 ao todo. Esquecer
`(none)` é um erro a menos que haja um `_`. `(some x)` também pode ser escrito, e vincula "algo não vazio".

Esta facilidade se aplica **exatamente** só a `Option<Sexpr>`. Para `Option<Option<Sexpr>>`, não ficaria claro
qual camada `(int n)` tirou, então escreva dois níveis de `match` como de costume.

Os mesmos padrões de downcast podem ser usados como estão sobre **um valor examinado que é um objeto trait
(`:dyn Trait`, capítulo 2)**: `match` o tira da caixa e então o entrega à maquinaria de padrões de `Sexpr` acima,
então não há sintaxe adicional. O conjunto de tipos que o implementam é aberto, então nunca pode ser exaustivo, e
`_` é obrigatório:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; decomposição por campos com o nome do tipo primeiro
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Inferência de tipos entre ramos**: todos os ramos precisam ter o mesmo tipo (exceto os ramos que divergem, como
com `panic`). Em um `match` escrito onde nenhum tipo é esperado, os ramos completam uns aos outros os argumentos de
tipo que faltam: `(result::ok v)` fixa só `T`, e `(result::err e)` só `E`, mas juntos fixam `Result<T,E>`. Um
argumento de tipo que nenhum ramo consegue fixar até o fim é um erro desse ramo (`cannot infer type argument ...`).
Fora de `match`, um argumento de tipo que não pode ser fixado é um erro na hora.

A verificação de exaustividade de um `match` que usa padrões de downcast não os conta para a cobertura das
próprias variantes de `Sexpr` (um `match` que lista só padrões de downcast precisa ser fechado com `_`). Para os
ADTs genéricos (`defstruct point<T> ...` etc.), os argumentos de tipo de um padrão de downcast não podem ser
inferidos, então a forma de decomposição por campos (`(point ...)`) e a forma de variante simples não podem ser
usadas; declare-os com `the`, como em `(the point<i32> p)`.

**Os downcasts também olham a instanciação.** Os argumentos de tipo explícitos são usados no casamento:
`(the point<i32> p)` só deixa passar valores de `point<i32>`, e um `point<string>` segue para o próximo ramo. Isso
porque um valor lembra seu tipo incluindo seus argumentos de tipo (o mesmo mecanismo que escolhe `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (com vínculos) se val casar com pattern; senão, els. defmacro
(while-let (pattern val) body...)   ; repete enquanto val (reavaliado a cada vez) casar com pattern. defmacro
```

## 5. Iteração

```lisp
(loop body...)                      ; um laço infinito. sai-se com break/return
(while test body...)                ; repete enquanto test for verdadeiro. defmacro
(until test body...)                ; repete enquanto test for falso (a negação de while). defmacro
(dotimes (var count-expr) body...)  ; avalia count-expr uma vez e percorre var de 0 a count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; iteração no estilo do CL com avanço em paralelo. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; a versão sequencial de do (vínculo let*, atribuição em ordem). defmacro
(doiter (var coll-expr) body...)    ; itera sobre um valor que implementa o trait Iter. defmacro

(break)                             ; sai só do laço mais interno. o valor é sempre Unit
(return)                            ; sai só do laço mais interno
(return value)                      ; sai do laço mais interno com um valor
```

Tanto `break` quanto `return` saem **só do laço envolvente mais interno** (não são um retorno antecipado da
função, e não podem atravessar a fronteira de uma `lambda`). O tipo de um `loop` é a junção dos tipos de valor dos
`break`/`return` encontrados dentro dele (`!` se nunca se sai). Para sair de uma função, use `return-from`, abaixo.

### 5.1 `block` / `return-from` — saídas nomeadas

```lisp
(block name body...)                ; um destino de saída nomeado. o valor é a última forma,
                                    ; ou o valor passado por return-from
(return-from name)                  ; sai daquele bloco com Unit
(return-from name value)            ; sai com um valor
```

**Cada função de `defun` / `defmethod` / `labels` estabelece implicitamente um bloco com o próprio nome** (como no
CL). Então `(return-from f v)` é um retorno antecipado da função:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` é uma saída **léxica**, e o nome é **resolvido onde é escrito**: o verificador associa um `return-from`
ao `block` envolvente e junta o tipo do seu valor ao tipo de saída do bloco. Então:

- Um `return-from` sem `block` correspondente é um **erro de tipo** (não um erro em tempo de execução).
- Um valor cujo tipo não encaixa nas outras saídas ou no tipo do corpo é um **erro de tipo** (a mesma regra dos
  ramos de `match`).
- Se blocos com o mesmo nome se aninharem, **o interno vence** (a regra de sombreamento do CL).
- **Não pode atravessar fronteiras de função.** De dentro de uma `lambda`, não se pode sair para um `block`
  externo (`lambda` não estabelece bloco: os blocos implícitos do CL precisam de um *nome*, e as funções anônimas
  não têm). O que precisa atravessar é `catch`/`throw` (capítulo 8, que é **dinâmico**).

Como `break`/`return` (capítulo 5), é uma saída **estática**, então no código compilado é um desvio para um bloco
básico fixado em tempo de compilação. Se houver um `unwind-protect` no meio, seu `cleanup` é executado
(capítulo 8).

Se você nunca escrever `return-from`, o bloco implícito não custa nada.

### 5.2 `loop` estendido (o LOOP do CL)

**Se o primeiro elemento de `loop` for uma palavra-chave**, ele é lido como uma sequência de cláusulas. Senão,
continua sendo o laço simples acima, e o significado dos `loop` existentes não muda (o mesmo que a regra do laço
simples do próprio CL).

O CL escreve as palavras de cláusula como símbolos simples (`(loop for i from 1 to 3 collect i)`), mas aqui
**todas são palavras-chave**: um `for` simples seria só uma referência de variável, e ser palavra-chave é também o
que o distingue de um laço simples. A exceção é `=`, que separa uma variável de um valor: sua posição não é
ambígua, então ele é lido tanto simples quanto como palavra-chave (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Cláusulas de variável** (escritas antes das cláusulas de corpo. É a regra do CL: escritas depois, poderiam ser
lidas como "iterar só daqui em diante", então é um erro):

| Cláusula | Significado |
|---|---|
| `:with v = e` | Vincula uma vez. Pode ler as variáveis de cláusulas anteriores |
| `:for v :in s` / `:for v :across s` | Os elementos de um `Iter` em ordem. A distinção lista/vetor do CL não existe aqui, então são duas escritas da mesma cláusula |
| `:for v :on s` | Os **sufixos** sucessivos. O CL passa o cons de cauda compartilhado, mas um `Iter` não tem cauda a compartilhar, então cada um é um `Vector` novo |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Contagem. `:downfrom`/`:upfrom` também funcionam |
| `:for v = e [:then f]` | Começa com `e`, e a partir da segunda vez usa `f` (sem `:then`, `e` toda vez) |
| `:repeat n` | Itera essa quantidade de vezes |

Com vários `:for`, eles avançam **em paralelo**, e o laço termina assim que qualquer um se esgota.

**Cláusulas de corpo** (executadas toda vez, na ordem escrita):

| Cláusula | Significado |
|---|---|
| `:do form...` | Pelos efeitos colaterais |
| `:collect e [:into v]` | Coleta em um `Vector<T>` |
| `:append e [:into v]` | Acrescenta o conteúdo de um `Iter` |
| `:sum e` / `:count e` | A soma / o número de vezes que foi verdadeiro |
| `:maximize e` / `:minimize e` | O máximo / o mínimo. **`Option<T>`** (assim como o CL devolve nil para uma sequência vazia; um tipo `Ord` arbitrário não tem elemento mínimo) |
| `:always e` / `:never e` | `true` se todos valerem; `false` imediatamente quando um falha |
| `:thereis e` | `e` é um **`Option<T>`**. Devolve o primeiro `some`, ou `none` se não houver (é isto que corresponde ao "primeiro valor não nil" do CL; para testar um `bool`, use `:always`/`:never`) |
| `:while e` / `:until e` | **Termina normalmente** aqui (`:finally` é executado, e o que foi coletado é a resposta) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Torna uma cláusula condicional |
| `:return e` | Sai imediatamente com esse valor (`:finally` não é executado, como no CL) |
| `:initially form...` / `:finally form...` | Antes do laço / na conclusão normal |

**`:named name`** (antes de qualquer outra cláusula, só uma vez) envolve o laço inteiro em `(block name …)`.
`(return-from name e)` consegue sair de uma vez mesmo de dentro de laços aninhados, e como `:return`, `:finally`
não é executado. Sem nome, nenhum bloco é estabelecido: o `loop` sem nome do CL estabelece `block nil`, mas aqui
não há `nil`, e `break`/`return` (capítulo 5) já oferecem "sair do laço mais interno".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Deixar de fora `:finally (return 0)` é um **erro de tipo**. São só as regras de `block` em ação (5.1): o tipo da
saída `int` não encaixa no `()` que o laço deixa quando se esgota.

**O valor do laço** é a acumulação da cláusula acumuladora, se houver (a primeira, se houver várias), `true` para
`:always`/`:never`, `none` para `:thereis`, e `()` se não houver nenhuma. Se a última coisa em `:finally` for
`(return e)`, esse é o valor: o idioma `finally (return …)` do CL, a única forma de um laço que não acumula dizer
a própria resposta.

**Diferenças em relação ao CL / o que não está incluído**:

- **As palavras de cláusula são palavras-chave** (acima).
- `:maximize`/`:minimize`/`:thereis` devolvem `Option<T>` (não há nil).
- **Escrever só `:return`, sem acumulação nem `:finally`, é um erro.** O CL devolve nil ao se esgotar, mas aqui não
  existe tal coisa, então o laço precisa dizer qual é seu valor ao se esgotar.
- Não estão incluídos juntar cláusulas paralelas com `:and`, `:being`/a iteração dedicada sobre tabelas hash,
  `:it` e `:nconc`.
- O tipo dos elementos de `:collect` vem do tipo da expressão acumulada. Tentar coletar um tipo que **não pode ser
  escrito como nome de tipo**, como um tipo de função, é um erro que diz isso.

## 6. Valores de função e chamadas

```lisp
(lambda (params) RetType body...)   ; cria um valor de função de primeira classe (uma closure)
(labels ((name (params) RetType body...) ...) body...)   ; definições de funções locais que podem ser mutuamente recursivas
(apply f arg1 ... argN rest-list)   ; chama f (uma função variádica com &rest), espalhando rest-list
```

As funções nomeadas também podem ser passadas como valores como estão (como argumentos de funções de ordem
superior etc.).

## 7. Outras formas especiais

```lisp
(setq var value ...)                ; a atribuição de variáveis do CL. só uma sequência de (setf var value). defmacro
(psetq var value ...)               ; atribuição em paralelo. avalia todos os valores primeiro e depois atribui. defmacro
(psetf place value ...)             ; psetq generalizado para lugares (a mesma expansão). defmacro
(setf place value)                  ; atribuição a um lugar. um lugar é um nome de variável / var::field /
                                     ; uma chamada da forma (accessor recv key...). válida se o
                                     ; tipo estático de recv tiver um método de instância chamado
                                     ; set-{accessor} (para o get de Vector<T> e HashTable<K,V>,
                                     ; set corresponde como exceção; senão, set-nome-do-acessor).
                                     ; o valor é o valor atribuído (como no CL). então
                                     ; em (if c (setf x 1) ()), then e else não têm tipos que batam
(incf place)  (incf place delta)    ; place += delta (delta=1 se omitido). o resultado é como com setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 se omitido)
(rotatef place1 place2 ... placeN)  ; gira N lugares (novo place1=antigo place2, ...,
                                     ; novo placeN=antigo place1). as subformas de cada lugar são avaliadas uma vez
(shiftf place1 ... placeN newvalue) ; desloca para a esquerda os valores de place2..N e põe newvalue em placeN.
                                     ; o valor de retorno é o antigo valor de place1
(list e1 e2 ... en)                 ; expande para (cons e1 (cons e2 (... ()))). () com zero argumentos.
                                     ; cada elemento é convertido implicitamente em Sexpr (como o cons do
                                     ; CL, pode conter qualquer valor). os escalares (int/i32/f64/ratio/
                                     ; char/bool/string/symbol) são envolvidos na variante Sexpr
                                     ; correspondente, e defstruct/defenum/Vector<T>/HashTable<K,V> e
                                     ; afins entram como estão (sem custo de conversão). o mesmo vale para
                                     ; os argumentos de &rest/format.
(source-file)                       ; o nome do arquivo de que esta forma foi lida (string). fixado
                                     ; como constante na verificação. corresponde ao *load-pathname* do CL,
                                     ; mas não é uma variável: os corpos de módulo são executados depois da
                                     ; verificação, então não se pode contar com "o que está sendo carregado agora",
                                     ; enquanto na verificação ele é sempre conhecido.
                                     ; para fontes que não são arquivos, o nome que o leitor lhes dá (<stdin>/<input>)
(quote datum)                       ; o mesmo que 'datum. devolve-o como dado Sexpr sem avaliar
(quasiquote template)               ; o mesmo que `template. embute expressões no modelo com ,/,@
(documentation name)                ; devolve a docstring de name (um nome simples ou Type::method) como Option<string>
(panic message)                     ; message: string. termina de forma anormal com um erro irrecuperável. tipo !
(unreachable)                       ; expande para (panic "unreachable"). defmacro
(todo)                              ; expande para (panic "todo"). defmacro
(as Type expr)                      ; conversão de tipo numérica/de caractere. as conversões que podem falhar causam panic se falharem
(try-as Type expr)                  ; como as, mas devolve o resultado como Option<Type> (None se falhar)
(print control args...)             ; expande o formato e escreve na saída padrão (sem quebra de linha)
(println control args...)           ; o mesmo (com uma quebra de linha no final)
(format dest control args...)       ; o format do CL. devolve a string expandida
(pprint x)                          ; imprime bonito. escreve primeiro uma quebra de linha, como no CL
(pprint-fill x)                     ; layout de preenchimento
(pprint-linear x)                   ; tudo em uma linha ou um elemento por linha
(pprint-tabular x [colinc])         ; layout tabular (16 colunas por padrão)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; construir um bloco lógico você mesmo
```

A família `print`/`println`/`format`/`pprint` são formas especiais, então seus argumentos variádicos (um único
objeto na família `pprint`) são envolvidos em `Sexpr` com seus próprios tipos antes de serem passados: é por isso
que `(println "~a" my-struct)` simplesmente funciona. Os detalhes das diretivas de formato e do pretty printer
estão em [Diretivas de formato](functions/format.md) e em [Impressão](functions/printing.md#4-o-pretty-printer).

`as`/`try-as` tratam só do catálogo numérico e de caracteres (entre `int`, os tipos inteiros de largura fixa e
`f32`/`f64`/`ratio`/`char`). O mesmo tipo não é conversão. **As conversões entre larguras inteiras (incluindo
`int`) e entre `f32`↔`f64` são conversões reais**: `as` trunca / arredonda, e `try-as` responde se cabe naquela
largura (precisão). `(as int x)` é a ampliação exata a partir de uma largura fixa, e `(as i32 n)` o truncamento a
partir de `int`. Inteiro → `char` pode falhar fora do intervalo, então `as` causa panic e `try-as` dá `None`.
Todo o resto (as ampliações, e o truncamento de `float->int`/`ratio->int`) sempre dá certo.
`float->int`/`ratio->int`/`char->int` chegam em `int`, e se for pedida uma largura mais estreita, `int->W` é
chamado depois. É uma facilidade de escrita que se expande para os métodos de conversão correspondentes
(`int->char`/`int->int`/`int->W` etc. em [Números](functions/numbers.md)).

`documentation`, como `quote`/`compile`, é uma forma especial que lê `name` sem avaliá-lo, como um símbolo simples
/ caminho `::` não avaliado. Ao contrário do `(documentation 'name 'function)` do CL, não recebe argumento de
tipo: resolve `name` na ordem variável → função → tipo → trait → macro (a mesma prioridade de quando um
identificador simples é avaliado como expressão) e devolve a docstring da definição encontrada
(`(documentation Type::method)` é para métodos). Não conseguir resolver (não há definição com esse nome) é um erro
na verificação; uma definição que existe mas não tem docstring dá `Option::none`. Tudo é decidido como constante na
verificação: nenhuma busca em tempo de execução acontece. Nomes livres qualificados por módulo (`mod::name`,
exceto `Type::method`) não são suportados.

## 8. Saídas não locais (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; executa body. se (throw 'tag v) acontecer em qualquer
                                    ; lugar que body alcance, esse v vira o valor
(throw 'tag value)                  ; sai para o (catch 'tag ...) dinamicamente envolvente mais próximo
(unwind-protect protected cleanup)  ; executa cleanup qualquer que seja a forma de sair de protected
```

Ao contrário de `break`/`return` (capítulo 5), esta é uma saída **dinâmica**: `throw` não procura lexicamente o
`catch` ao seu redor, e alcança um `catch` com a mesma etiqueta através de qualquer número de chamadas de função.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; se não for encontrado, o valor do final como de costume
```

- **As etiquetas são só símbolos literais** (`'done`). Ao contrário do CL, não são avaliadas.
- **Uma etiqueta carrega um tipo.** O tipo é decidido na primeira vez em que `'tag` é usado, e todo
  `throw`/`catch` posterior do mesmo símbolo é confrontado com ele. Usá-la com outro tipo é um erro de tipo.
- O tipo de `throw` é `!` (diverge). O tipo de `(catch 'tag expr)` é a junção do tipo de `expr` e do tipo da
  etiqueta.
- O valor de `unwind-protect` é o valor de `protected`. O valor de `cleanup` é descartado. `cleanup` é executado
  qualquer que seja a forma de sair de `protected`: além da conclusão normal, `throw` e `panic`, também é executado
  quando se sai por `break`/`return`/`return-from`. Uma saída não local do próprio `cleanup` vence a saída em
  andamento.
- Os `unwind-protect` aninhados são executados de dentro para fora. Um `break` que sai de um laço **dentro** de
  `protected` não saiu de `protected`, então seu `cleanup` não é executado.

As condições do CL (`define-condition`/`handler-bind`/`invoke-restart`) não são adotadas. Elas não combinam com a
tipagem estática, então as falhas recuperáveis são expressas com `Result` (capítulo 9).

## 9. Política de tratamento de erros

- Falhas recuperáveis: `Result<T,E>` + `match`. Falhas irrecuperáveis (bugs, invariantes quebradas): `panic`.
- Não há sintaxe correspondente a `?`/try. As ramificações são escritas explicitamente com `match`.
- Os nomes de funções e de formas especiais não usam `!` (operações destrutivas) nem `?` (predicados) como sufixos.
  Os predicados são nomeados com um sufixo `-p`/`p` (`zerop`, `consp` etc.) ou com um prefixo `is-` (`is-some`,
  `is-ok` etc.).

## 10. Compilação

```lisp
(compile name)                      ; compila com JIT para código nativo um defun/método já definido
(compile-file src-path out-path)    ; compila com AOT um arquivo-fonte para um executável nativo (pula o `(main)` final)
(dump path)                         ; grava o ambiente atual (informação de tipos + corpos compilados) em um arquivo
(disassemble name)                  ; imprime no que essa definição se transforma (código de máquina do host por padrão, LLVM IR com true como segundo argumento)
```

`compile` é uma forma especial; `name` não é avaliado e é lido como um símbolo simples / caminho `::` não avaliado
(uma string é um erro de tipo). As funções genéricas não podem ser alvo: em cada lugar de uso é criada uma cópia
para cada tipo, então não existe um único corpo compilado. **Um nome que não pode ser resolvido é um erro na
verificação** e nunca é levado até o tempo de execução (há mensagens separadas para: o tipo existe mas não esse
método / nem o tipo nem a função existem / um nome simples não definido). A visibilidade aqui é tratada como em
qualquer outra referência: "existe mas não é visível daqui" falha na verificação, assim como "não se resolve".

As funções chamadas também são compiladas transitivamente, então **uma função que (mesmo indiretamente) chama algo
que não pode ser compilado não pode ser compilada**. O processo não cai; ela é recusada com um erro que diz isso.
Todas as funções embutidas podem ser compiladas, então as únicas funções recusadas desse jeito são as que chamam as
seguintes operações só do interpretador:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

As que são só do interpretador são `compile`/`compile-file`/`dump` e `trace`/`untrace`/`step`/`disassemble`
([Ferramentas da implementação](functions/system.md#5-ferramentas-da-implementação-clhs-252)). Mais do que coisas
que não podem ser compiladas, são operações do lado que compila (o que `dump` grava é o próprio ambiente do
interpretador, que um executável AOT não tem; o que `trace` observa e onde `step` para são os caminhos de chamada
do interpretador em execução; e `disassemble` usa o próprio compilador). `room`/`dribble`/`ed` não estão entre
elas e podem ser compiladas normalmente.

O que **pode** ser compilado: E/S de streams e arquivos, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, as funções transcendentes, as
operações de bits, `catch`/`throw`/`unwind-protect`, os quatro `eq`/`eql`/`equal`/`equalp` (o que permite
compilar `case` para todo tipo), toda a família de impressão incluindo `print`/`println`/`format`/`pprint` e
`pprint-logical-block`, `read` e `eval`. A biblioteca padrão é distribuída já compilada.

Um executável AOT contém só os recursos que o programa usa. Um programa que não imprime não leva o motor de
formatação, um que não chama `read` não leva o leitor, e um que não chama `eval` não leva verificador nem
interpretador.

Na linha de comando, `typl -c src-path [-o out-path]` (`-c` também pode ser escrito `--compile`) faz o mesmo que
`compile-file`. Sem `-o`, a saída é `src-path` sem a extensão `.typl`. Por padrão, a biblioteca estática
`libtypelisp_front.a` ligada aos executáveis é, para uma build de release do `typl`, a que o `typl` carrega dentro
de si, gravada na primeira ligação em `$TYPELISP_HOME/lib/<ID da build>/` (ou em `~/.typelisp/lib/<ID da build>/`
sem `TYPELISP_HOME`) e usada a partir dali; para uma build de depuração, a do lugar onde o `typl` foi compilado.
`typl --remove-lib` apaga o que esse `typl` gravou. Com `--others`, apaga as de outros IDs de build; com `--all`,
as de todos os IDs de build. Com `typl --lib-dir DIR`, usa-se a de `DIR` (tanto para `-c` quanto para
`compile-file`), e se ela não estiver lá, é um erro na inicialização.

### 10.1 Dumps

```lisp
(dump "session.typld")     ; gravar um
```
```sh
typl --image session.typld prog.typl   # iniciar a partir dele
typl --image session.typld             # o REPL também
```

Um dump guarda informação de tipos e corpos compilados em um arquivo. O que `(dump path)` grava é o que a sessão
atual carregou (a biblioteca padrão, ou um dump passado com `--image`) mais **o que a própria sessão definiu**.
Então a saída é autocontida, e `typl --image` sobe o mesmo ambiente. O que a sessão compilou com `(compile f)` é
gravado na forma compilada.

O que é salvo são **definições, não histórico**:

- As expressões de nível superior da sessão (`(println ...)` etc.) não são incluídas. Seria um problema se o
  carregamento as executasse de novo.
- As variáveis globais voltam com **o valor do seu inicializador executado de novo**, não com o valor do momento
  do dump. É uma diferença proposital em relação ao `save-lisp-and-die` do SBCL (que grava o heap como está), e
  essa escolha faz desaparecer toda uma família de problemas: os "valores que não podem ser salvos", como streams
  abertos, ponteiros de função de closures e memória externa.
- Ao contrário de `save-lisp-and-die`, **o processo não morre**, já que gravar não danifica a imagem.

Um dump registra as versões da biblioteca padrão e do compilador da implementação que o gravou. Carregá-lo com um
`typl` de outra versão é um erro; ele nunca é aceito silenciosamente.

### 10.2 `eval` em executáveis AOT

`eval` verifica os tipos contra "o ambiente global atual" e depois avalia
([Análise e avaliação](functions/system.md#6-análise-e-avaliação)). Esse ambiente (as tabelas de assinaturas, tipos
e macros que o verificador consulta, e os corpos que o interpretador consegue executar) **não está no código de
máquina**. Uma função compilada não passa de um símbolo colocado em um endereço; ela não tem nem os tipos dos seus
argumentos nem uma tabela para procurar corpos pelo nome.

Então, só para os programas que chamam `eval`, `compile-file` **monta esse ambiente em tempo de compilação e o
grava no executável**. O formato é o mesmo de um dump, contendo a parte da biblioteca padrão e a parte do próprio
programa. Tudo o que acontece na inicialização é restaurá-lo: o código-fonte não é relido, e nada é verificado de
novo. Nada é acrescentado aos programas que não chamam `eval`.

Consequências:

- **A inicialização demora mais e o executável é maior**, já que entram o código do verificador e do interpretador
  e um instantâneo do ambiente. O heap também fica um pouco maior.
- **As formas passadas a eval são interpretadas.** Mesmo quando a forma passada a eval chama as próprias funções
  do programa, o que é executado é o corpo interpretável que o instantâneo guarda. O resultado é o mesmo; só a
  velocidade difere.

O armazenamento das variáveis globais é **compartilhado** com o código compilado (os mesmos slots). O inicializador
de um `defvar` é executado uma vez pela inicialização compilada, e a restauração o pula, então um inicializador com
efeitos colaterais não é executado duas vezes.

`compile-file` também lê a biblioteca padrão (e embute os corpos dela no executável), então funções da biblioteca
padrão como `abs`/`gcd`, e `(impl print-object ...)` assim como `(defmethod print-object ...)`, podem ser usadas com
AOT.

`compile-file` também aceita `use` (e `import`/`shadowing-import`). O `(use m)` do arquivo de entrada encontra os
arquivos pelas mesmas regras de `typl file.typl`, e os arquivos de dependência encontrados também são compilados e
ligados ao executável: uma organização em que `main.typl` lê `http.typl` por meio de `(use http)` pode ser
compilada com AOT como está. As próprias definições do arquivo de entrada também vão para o módulo com o nome do
arquivo, como com `typl file.typl` (`point` em `p.typl` é `p::point`). Então a representação impressa dos valores
(`#<p::point x: 1 y: 2>`) é a mesma qualquer que seja a forma de executar.

## 11. Macros de leitura (readtable)

O que o leitor **faz quando encontra certo caractere** pode ser substituído a partir do programa (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f lê o caractere c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f lê a sequência de dois caracteres d s
(get-dispatch-macro-character d s)    ; Option<f>
```

O tipo de `f` é `(fn (string-input-stream char) Option<Sexpr>)`. O primeiro argumento é **um stream sobre o texto
ainda não lido**, e o segundo é **o caractere que o disparou** (o segundo caractere em um despacho). O valor de
retorno vira o dado lido naquele ponto. O stream é um tipo concreto em vez de `:dyn PeekInput` porque o leitor
sempre passa este único tipo: `read-sexpr` / `read-char` / `peek-char` / `unread-char` / `read-delimited-list`
recebem todos `(where (PeekInput S))`, então todos funcionam sobre o tipo concreto como está.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => lido como (not (equal 1 2)), isto é, true
```

O leitor **olha os caracteres de macro antes da sintaxe embutida**, então pode tomar conta também de `(` e `'`. Os
subcaracteres de `#` registrados assim têm prioridade sobre os `#b`/`#x`/`#.` embutidos. Um caractere diferente de
`#` vira um caractere de despacho na hora quando passado a `set-dispatch-macro-character`: **não** há equivalente
ao `make-dispatch-macro-character` do CL. O registro já faz o trabalho, então um passo separado não teria nada a
fazer.

**Quando passam a valer** depende do caminho de leitura, assim como `#.` (capítulo 1):

- O REPL e `(load ...)` executam uma forma por vez, então **as funções definidas em formas anteriores** podem ser
  registradas como estão.
- Os arquivos de módulo são verificados como uma unidade e executados depois, então **só as chamadas a
  `set-macro-character` / `set-dispatch-macro-character` são executadas imediatamente** (o papel do
  `(eval-when (:compile-toplevel) ...)` do CL). Como são executadas imediatamente, **a função passada já precisa
  existir naquele ponto**. Um `defun` do mesmo arquivo ainda não foi executado, então escreva uma `lambda`, ou use
  a biblioteca padrão ou algo que já foi executado. Só as chamadas de nível superior são consideradas; ele não olha
  dentro de `progn` nem de `let`.

Os `read` / `read-from-string` embutidos também consultam a readtable (como no CL).

**O que não existe**: `*readtable*` e `copy-readtable`, e `readtable-case`. Os dois primeiros porque uma readtable
**não é um valor**: um valor teria de ser "algo que pode ser entregue a um leitor", mas o leitor que lê o
código-fonte está fora do programa, sem lugar para onde entregá-la. `readtable-case` porque o capítulo 1 decide que
o leitor desta linguagem sempre passa para minúsculas (o `:downcase` do CL).


## 12. Concorrência (tarefas)

**Uma tarefa é uma thread leve** (nos termos do Go, o que um comando `go` inicia) e é executada de forma cooperativa
(não há preempção). A troca não passa pelo kernel, e o estado de execução vive no heap e não em uma pilha de
máquina, então criar tarefas em grande número é barato.

**As tarefas são executadas ao mesmo tempo em várias threads de SO** (paralelismo multinúcleo). O número de threads
é a variável de ambiente `TYPELISP_THREADS` (o total, incluindo a thread que executa `main`; o padrão é o
paralelismo da máquina). No `typl`, **só as tarefas compiladas** são executadas em outras threads, e as tarefas
interpretadas são executadas na thread do interpretador (12.7). Os dados compartilhados passam por `Mutex<T>` ou
`Chan<T>`; leituras e escritas simultâneas que não passam por eles são indefinidas, como no Go (12.7).

Do vocabulário, **só `task` / `thread` / `select` são formas especiais**; o resto são funções, métodos e macros
comuns ([Tarefas e canais](functions/concurrency.md)).

### 12.1 `task` — iniciar uma tarefa

```lisp
(task (f arg...))                   ; devolve Task<T>, em que T é o tipo de retorno de f
```

**Só aceita a forma de uma chamada.** `f` e cada `arg` são avaliados onde o `task` é escrito, na ordem escrita, e
na nova tarefa só acontece **a chamada**. É a mesma regra do `go f(x)` do Go, e também o motivo pelo qual ele aceita
uma forma de chamada em vez de um thunk: um thunk capturaria seus argumentos sem avaliá-los.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i é avaliado na hora a cada vez; sem armadilha de captura

(task ((lambda () ()                ; para executar um corpo arbitrário, chame uma lambda
         (println "start")
         (send ch 1))))
```

As formas especiais (`if` / `let` / `progn` …) não podem ser escritas diretamente sob `task`.

**Por que não pode ser uma função**: escrever `(spawn (lambda () T body...))` exigiria escrever `T`, já que
`lambda` exige uma anotação de tipo de retorno, e uma macro não sabe o tipo de retorno de `(f a b)`. Só o
verificador sabe.

### 12.2 `thread` — iniciar uma tarefa em uma thread de SO dedicada

```lisp
(thread (f arg...))                 ; devolve Thread<T>, em que T é o tipo de retorno de f
(join th)                           ; espera a conclusão e devolve seu valor (quantas vezes quiser)
```

A forma e as regras de avaliação são as mesmas de `task` (só aceita uma forma de chamada, e `f` e `arg` são
avaliados onde é escrito). A diferença é onde ela é executada: **inicia uma thread de SO dedicada àquela tarefa e é
executada só nela**. Ela não é multiplexada com outras tarefas, então chamar dentro dela uma função C bloqueante
(`defffi`) para só aquela thread, e as outras tarefas avançam. Dentro dela, `task`, `send`, `recv` e o resto podem
ser usados como estão.

- `Thread<T>` é a contrapartida de `Task<T>`. Como `wait`, `join` para **a tarefa que chama**, e o valor fica em
  cache. Quando a tarefa termina, a thread também termina.
- As regras de panic são as mesmas de `task` (o processo inteiro cai). Quando `main` retorna, o processo termina.
- Para escrevê-lo como função, use `(Thread::spawn (lambda () T body...))` (o `std::thread::spawn` do Rust). Também
  é possível passar uma função nomeada.
- **Em uma thread dedicada só é executado código compilado.** Quando o `typl` avalia `(thread (f ...))` ou
  `Thread::spawn` enquanto interpreta, ele compila na hora a função a executar (e o que ela chama) antes de
  executá-la. O que não pode ser compilado (uma `lambda` que se refere a variáveis locais de fora, construir uma
  estrutura etc.) é, antes de a thread ser iniciada, um panic tratado do mesmo jeito que um `(panic ...)`. Uma
  `lambda` que se refere a variáveis locais pode ser passada se for criada dentro de uma função compilada.

### 12.3 `select` — esperar várias operações de canal ao mesmo tempo

```lisp
(select
  ((v (recv ch1)) body...)          ; um ramo de recepção. v é vinculado a um Option<T>
  ((send ch2 x) body...)            ; um ramo de envio
  (else body...))                   ; opcional. **se escrito, vai por último**
```

- **Com `else`, não bloqueia** (o `default` do Go). Sem ele, espera até que um se torne possível.
- **Se vários forem possíveis ao mesmo tempo, um é escolhido ao acaso** (na ordem escrita, os ramos posteriores
  passariam fome).
- O `v` de um ramo de recepção é um **`Option<T>`**. Um canal fechado é "uma resposta", não um motivo para pular o
  ramo, então faça `match` sobre ele dentro do ramo.
- O tipo é **a junção dos tipos dos corpos de todos os ramos** (a mesma regra dos ramos de `match`).
- `(select)` com zero ramos é um erro de tipo (o `select{}` do Go, que bloqueia para sempre, não é adotado). Um
  `select` só com `else` também é, já que é o mesmo que escrever o corpo diretamente.

**As expressões de canal e os valores a enviar são avaliados uma vez cada, da esquerda para a direita, qualquer
que seja o ramo escolhido** (a mesma disciplina que `case` tem com suas chaves).

```lisp
(select                             ; receber com tempo limite
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([um canal que entrega após um tempo](functions/concurrency.md#5-after--um-canal-que-entrega-após-um-tempo))
é "um canal que entrega um valor depois de `sec` segundos", correspondendo ao `time.After` do Go.

### 12.4 Interação com outros recursos

| Recurso | Como se relaciona com as tarefas |
|---|---|
| `catch` / `throw` | **Não atravessam as fronteiras das tarefas.** Um `throw` que tenta sair do corpo de uma tarefa é um panic |
| `unwind-protect` | A limpeza é executada quando uma tarefa termina naturalmente. **Não é executada quando o processo termina porque a tarefa principal terminou** |
| `block` / `return-from` | Léxicos, então não atravessam fronteiras de `lambda` |
| `panic` | Como no Go, o processo inteiro cai. `wait` não observa um panic como valor |
| `dlet` | **Não é um vínculo por tarefa.** Continua "pegando emprestada e devolvendo uma global", então as tarefas interferem umas nas outras |
| Saída padrão | Compartilhada por todas as tarefas. A saída de um `println` nunca se mistura com outras no meio de uma linha |
| `compile` / `eval` | Sem restrições. `(compile f)` dentro de uma tarefa funciona |

### 12.5 Onde as tarefas alternam

O escalonamento é cooperativo, então **as tarefas só alternam onde você escreve**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **as operações de canal que precisam esperar** (`send`/`recv`/`select`), e **as operações de socket
que precisam esperar** (`accept` / `tcp-connect` (incluindo a resolução de nomes) / ler e escrever sockets /
`recv-from`; [Rede](functions/network.md)). Todos os sockets são não bloqueantes: se um não estiver pronto, só
aquela tarefa para, e ela retoma quando o SO diz que está pronto, a mesma forma do netpoller do Go. Só quando
nenhuma tarefa pode ser executada a implementação espera o SO até o prazo de `sleep` mais próximo.

As operações de canal que podem responder na hora (um `send` com espaço no buffer, um `recv` com um valor
esperando, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **não gastam a vez**. Isso significa que você não é
interrompido inesperadamente por uma leitura, e isso é tratado de forma diferente de `(sleep 0.0)`, que é o
"ceder por 0 segundos" do CL.

**Não há preempção.** Um laço apertado que não chama nada deixa as outras tarefas sem vez. Porém, os laços
compilados passam o controle ao escalonador periodicamente, então um laço apertado compilado não as deixa sem vez.

### 12.6 Código compilado e tarefas

O código compilado também pode suspender tarefas. O mesmo vale para os executáveis criados com `compile-file`:
`main` é executada como a tarefa principal do escalonador, e `task`, `sleep`, `wait`, os canais e as esperas de
socket funcionam todos com o mesmo significado que no `typl`. Quando `main` retorna, o processo termina e as
tarefas restantes são interrompidas (como no Go). O interpretador nunca é posto no executável por causa do
escalonador.

A única exceção é "dentro de um callback do FFI de C", onde as operações que **precisariam esperar** são erros (mais
amigáveis do que um deadlock silencioso): enquanto uma função passada com `defffi` está sendo chamada pelo C, a
pilha do C está por cima, e não há como suspender a tarefa e retomá-la depois.

Os lugares a seguir também são funções chamadas no meio de uma tarefa, e mesmo assim não podem suspender: os
métodos `print-object`, `~/name/` em `format`, as macros de leitura, o interior de `eval` e os inicializadores de
`defvar` nos executáveis AOT. Aqui, **as operações que respondem sem esperar passam** (`(recv ch)` com um valor no
buffer, `read-line` em um socket com dados já recebidos, `(task ...)`, `(yield)` etc.), e **as operações que
realmente precisariam esperar são erros** (não param o processo na hora, mas são um panic como
`` `recv` cannot block: ... ``, tratado do mesmo jeito que um `(panic ...)`).

### 12.7 Diferenças em relação ao Go

- **No `typl`, só as tarefas compiladas vão para outras threads.** O estado do interpretador não pode ser
  compartilhado entre threads, então as tarefas de um `task` interpretado são executadas na thread do
  interpretador. Uma tarefa compilada também **se move para a thread do interpretador e fica lá** (não volta) no
  ponto em que chama um valor de função interpretado, chama um método `:dyn` que ninguém compilou, ou chama
  `eval`/`macroexpand`/`read`. Se um cálculo longo tocar código interpretado ao menos uma vez pelo caminho, o resto
  é executado na thread do interpretador.
- **No `typl`, os workers vivem só durante uma avaliação de nível superior.** Enquanto o REPL espera a entrada, e
  entre as formas de nível superior, as outras threads não avançam as tarefas (as tarefas restantes continuam de
  onde pararam na próxima avaliação). No fim de uma avaliação, ele espera cada thread terminar seu passo atual,
  então se uma função C (`defffi`) continuar bloqueando dentro de uma `thread`, a avaliação não termina até que ela
  retorne.
- **Impressão nos workers**: os métodos `print-object` / `~/name/` interpretados não podem ser executados em outras
  threads, então imprimir esses valores em outra thread é um panic tratado do mesmo jeito que um `(panic ...)`
  (`(compile T::print-object)`, ou imprima a partir da tarefa principal).
- **As condições de corrida de dados são indefinidas** (a mesma posição do Go). O resultado de várias tarefas
  mudarem o mesmo valor sem passar por `Mutex<T>` / `Chan<T>` não é garantido.
- **`task` devolve um valor.** Ao contrário do comando `go` do Go, ele devolve um `Task<T>`, e `(wait t)` obtém o
  resultado.
- **Não há canais nil.** O idioma de fan-in do Go (definir como `nil` um canal fechado para tirá-lo dos ramos de
  `select`) não pode ser escrito, então inicie uma tarefa por entrada e reúna-as com um `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--esperar-n-conclusões)). Também é o jeito recomendado no Go,
  mas é **a primeira diferença com que esbarra quem vem do Go**.

<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Tempo, ambiente e implementação

Funções de tempo, consultas sobre o ambiente de execução, ferramentas da implementação, análise e avaliação de
texto, docstrings e macros.

## 1. Tempo

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Dois campos: `day` (dias desde 1900-01-01) e `second` (o segundo dentro daquele dia, 0..86399) |
| `internal-time` | — | `defstruct` | Dois campos: `second` e `microsecond` (dentro daquele segundo, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | O tempo desde a época do CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | O tempo decorrido relativo ao processo |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | O **tempo de CPU** que este processo usou (usuário mais sistema) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Como número de segundos. A forma de relatar a diferença entre duas leituras |
| `internal-time-units-per-second` | — | `int` | `1000000` (microssegundos), a unidade do campo `microsecond`. Como no CL, o valor é escolha da implementação |
| `time` | `(time form)` | Macro | Executa `form`, imprime o tempo real e o tempo de CPU em uma linha cada, e devolve o valor de `form` como está |

O tempo real e o tempo de CPU dizem coisas diferentes. Em um trabalho que espera principalmente por E/S, os dois
diferem muito, e essa diferença é justamente o que você quer saber, então `time` mostra os dois.

`sleep`, que para uma tarefa, está em [Tarefas e canais](concurrency.md#3-yield--sleep--ceder-a-vez).

## 2. Decodificar e codificar datas

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Nove campos**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Os nove valores de retorno do CL como uma estrutura (não há valores múltiplos) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Tempo universal em componentes de calendário. `zone` são horas a oeste de Greenwich (a mesma direção do CL). **Se omitido, é o horário local** (como no CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | O inverso. Sem `zone`, os argumentos são lidos como **horário local** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | O momento atual, decodificado no horário local |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | O deslocamento do horário local a oeste de Greenwich, em **segundos**, naquele tempo universal |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Se o horário de verão estava em vigor naquele tempo universal |

Como no CL, em `day-of-week` **0 é segunda-feira e 6 é domingo**.

**Sem `zone`, usa-se o horário local**, como no CL. O deslocamento local é perguntado ao SO, então o resultado
depende de onde a máquina está. **Dar uma zona explícita o torna determinístico**, e `0` é UTC.

A unidade de `zone` é, como no CL, "horas a oeste de Greenwich", então UTC+9 se lê como `-9`. No entanto, **o
argumento é um inteiro e o campo `zone` do resultado é um `f64`**. Os deslocamentos reais nem sempre são horas
inteiras (a Índia é +5:30, o Nepal +5:45), e arredondar o valor relatado contaria uma mentira silenciosamente.
Uma zona que você escreve à mão é um número inteiro de horas, então o argumento é `int`.

Quando `zone` é dado, `daylight-p` é `false` e `zone` é exatamente o valor dado, como o CL especifica
(*If a time-zone is supplied, daylight saving time information is ignored*).

Um horário local que cai dentro de uma transição de horário de verão não é único, para começar, e o CL não diz
qual escolher. `encode-universal-time` devolve uma das duas respostas para esse horário.

## 3. O ambiente de execução

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | A linha de comando. **O elemento 0 é o nome do programa** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Uma variável de ambiente. `none` se não estiver definida ou não for UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. A base de `user-homedir-pathname` ([Nomes de caminho](streams-files.md#92-funções)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | A versão da implementação |
| `machine-type` | `(machine-type)` | `()→string` | A arquitetura de CPU (`x86_64` / `aarch64` …). O valor do **alvo da build** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | O nome do host |
| `machine-version` | `(machine-version)` | `()→Option<string>` | O nome do hardware **em execução agora** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` onde não pode ser determinado |
| `software-type` | `(software-type)` | `()→string` | O SO (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | A versão do SO (`uname -r`, por exemplo `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Um nome curto para o local de instalação. **Sempre `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Igualmente, um nome longo. **Sempre `none`** |

As que devolvem `Option` são itens para os quais o CL permite `NIL` (*or nil if no such name can be
determined*). O POSIX não tem onde registrar nomes de site, então elas são sempre `none`; o SBCL devolve o
mesmo. Note a diferença entre `machine-type` e `machine-version`: o primeiro é a arquitetura para a qual este
binário foi **compilado**, o segundo é o chip que o **executa** agora.

O elemento 0 de `command-line-args` é o caminho do script para `typl script.typl a b`, e o próprio executável
para um executável AOT executado como `./prog a b`. **Qualquer das duas formas de executar lê os mesmos
argumentos nos mesmos índices** (o `typl` retira seu próprio nome e opções como `--heap-cells` antes de
repassá-los).

## 4. Perguntar ao usuário

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Aceita um único `y` / `n`. Pergunta de novo até obter um |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Faz o usuário escrever por extenso `yes` / `no`. Para perguntas em que um engano sai caro |

Ambas leem de `*standard-input*`. Só o fim da entrada para as repetições, e então o resultado é `false`.

## 5. Ferramentas da implementação (CLHS 25.2)

A camada em que a implementação responde perguntas sobre si mesma. `heap-info` / `room` / `dribble` são funções
comuns; `trace` / `untrace` / `step` / `disassemble` / `ed` são **formas especiais** (`trace` / `untrace` /
`disassemble` / `ed` recebem o *nome* de uma definição, e `step` uma *forma*, tudo sem avaliar).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | O estado atual do heap como estrutura. Os mesmos números que `room` imprime |
| `room` | `(room &optional verbose)` | `(bool)→()` | Relata `heap-info` em `*standard-output*`. `(room true)` dá mais detalhes |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Começa a gravar a saída da sessão em `path` / para de gravar quando chamado sem argumento |
| `trace` | `(trace name...)` | `Sexpr` | Relata em `*trace-output*` as chamadas às definições nomeadas. Devolve a lista de nomes rastreados agora |
| `untrace` | `(untrace name...)` | `Sexpr` | Para de relatar. **Sem argumentos, remove todos** |
| `step` | `(step form)` | O tipo de `form` | Avalia `form`, parando em cada chamada para perguntar |
| `disassemble` | `(disassemble name [llvm])` | `()` | Imprime no que essa definição se transforma. O código de máquina do host por padrão, LLVM IR com `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Inicia `$VISUAL` / `$EDITOR`. Com um nome, abre a linha em que essa definição está escrita |

`trace`/`untrace`/`step`/`disassemble` são só do interpretador, e as funções que as chamam não podem ser
compiladas ([capítulo 10 da Referência de sintaxe](../syntax.md#10-compilação)).

### 5.1 Campos de `heap-info`

| Campo | Tipo | Conteúdo |
|---|---|---|
| `capacity` / `live` / `free` | `int` | A arena de cons inteira e sua divisão. Sempre `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | As contagens atuais dos outros três tipos de objeto do heap |
| `gc-count` | `int` | O número de coletas desde que a implementação iniciou |
| `growable` | `bool` | Se a arena ainda pode crescer |

Os campos são todos `int` (exceto `growable`). O limite de crescimento (veja a descrição de
`typl --heap-cells`) não é relatado, porque o que quem lê quer saber é se ela ainda pode crescer (`growable`).

### 5.2 O que `trace` / `step` conseguem e não conseguem ver

- **As definições com corpos compilados também são visíveis, a partir de pontos de chamada que estão sendo
  interpretados.**
- **Os pontos de chamada *dentro* do código compilado não são visíveis.** Rastrear um nome que tem um corpo
  compilado acrescenta uma nota de uma linha dizendo isso. A mesma limitação que o SBCL descreve para chamadas
  locais.
- **As chamadas por meio de valores de closure (`funcall`/`apply`) não são visíveis.** As closures não têm nome.
- **As definições genéricas não estão incluídas.** Em cada lugar de uso é criada uma cópia para cada tipo, então
  não há um único corpo a nomear (o mesmo motivo, e a mesma redação, de quando `compile` recusa).

Os comandos de `step` são `s` (entrar nesta chamada; uma linha vazia faz o mesmo), `n` (pular esta chamada),
`c` (parar de perguntar daqui em diante) e `q` (abortar). **Se a entrada padrão não for um terminal, `step`
simplesmente avalia `form`**: um comportamento degenerado que o CLHS permite explicitamente, para que scripts e
testes não fiquem presos em um prompt que ninguém pode responder.

O `$VISUAL` / `$EDITOR` de `ed` é dividido nos espaços em branco, então `EDITOR="code -w"` funciona. Se nenhum
estiver definido, o resultado é `Err`: ele não adivinha `vi`. O número da linha é passado primeiro, na forma
`+N`.

`dribble` grava as três vias pelas quais a saída da sessão sai do processo: o que `print`/`println`/`format`
escrevem, o que é escrito em streams conectados à saída padrão, e as linhas digitadas no REPL junto com os
valores que o REPL imprime de volta.

## 6. Análise e avaliação

Todas estas lidam com texto e dados do tempo de execução (que o próprio programa não controla), então em caso
de falha devolvem o `Err` de um `Result` em vez de entrar em panic. Os tipos de erro são tipos concretos por
operação ([Tipos de erro](option-result.md#3-tipos-de-erro-e-o-trait-error)).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | O `parse-integer` do CL. Pula os espaços em branco iniciais e finais (o mesmo conjunto de `trim`), lê no máximo um sinal `+`/`-` e depois dígitos na base `radix` (10 por padrão, de 2 a 36; os dígitos acima de 10 em maiúsculas ou minúsculas). Não há limite de dígitos (`int`). Quaisquer outros caracteres sobrando dão `Err`. Com `:junk-allowed true`, para no primeiro não dígito e ignora o resto, mas dá `Err` se não houver nem um dígito (corresponde ao `nil` do CL). Não devolve o segundo valor do CL (a posição em que a leitura terminou). Um `radix` fora do intervalo causa panic (um erro de quem chama, não do texto) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Um número de ponto flutuante. Também aceita `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Lê um `Sexpr` de `s` (com o mesmo leitor que lê o código-fonte). Parênteses desbalanceados, strings não terminadas e afins dão `Err`. Ler de um stream é `read-sexpr` ([Streams](streams-files.md#6-funções-genéricas-e-operações-com-arquivos)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` mais **a posição em que a leitura terminou**. `(car r)` é o valor e `(cdr r)` a posição do próximo caractere a ler. `start` é 0 por padrão |
| `read-from-string-preserving-whitespace` | Igual ao acima | Igual ao acima | O mesmo, mas não consome o espaço em branco que terminou o dado. A diferença aparece na posição devolvida |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Verifica os tipos de `form` em tempo de execução e a avalia. Segue o `eval` do CL |

O CL devolve **dois valores** (o valor e a posição) de `read-from-string`, mas esta linguagem não tem valores
múltiplos, então devolve um `cons-cell`. Ter a posição faz de ler uma string dado por dado um laço em vez de
uma nova varredura:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

A diferença que `preserving-whitespace` faz é **um caractere de espaço em branco**: o `read` do CL consome o
espaço que terminou o dado, e `read-preserving-whitespace` o deixa. `(read-from-string "12 34")` devolve a
posição 3, e a versão que preserva devolve 2.

A sintaxe numérica que o leitor aceita está no [capítulo 1 da Referência de sintaxe](../syntax.md#1-elementos-léxicos).
O que `*print-radix*` ([Impressão](printing.md#62-base-caixa-e-legibilidade)) imprime pode ser lido de volta
como está. Não existe o `*read-base*` do CL.

### 6.1 O que `eval` significa

Segue o `eval` do CLHS: avalia no **ambiente global atual** (funções, variáveis, tipos e macros globais,
incluindo as definições acrescentadas em tempo de execução) e no **ambiente léxico nulo** (os vínculos locais
dos `let`/`lambda` de quem chama não são visíveis). Tanto expressões quanto definições
(`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) podem ser avaliadas, e as definições são registradas no
ambiente global imediatamente e de forma permanente.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; o x global é visível
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; devolve o nome definido
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; a definição recém-feita é visível
```

- **Valor de retorno**: para uma expressão, o resultado como `Option<Sexpr>`; para uma definição, o símbolo do
  nome definido (como no CL). Para usar o resultado, desmonte o `Sexpr` com `match` (`(int n)`/`(str s)`/…).
- **Diferenças devidas aos tipos estáticos (importante)**: o CL devolve o valor real do resultado, mas nesta
  linguagem o tipo de retorno só pode ser uniformemente `Result<Option<Sexpr>,EvalError>`. Além disso, **o
  código escrito estaticamente não pode se referir antecipadamente a nomes que `eval` define em tempo de
  execução**: um `(sq 9)` escrito diretamente em um arquivo é verificado antes que o `eval` que define `sq` seja
  executado, e fica "indefinido". No entanto, **os `eval` posteriores o enxergam** (a verificação de tipos deles
  roda em tempo de execução, depois da definição). O REPL verifica e executa uma linha por vez, então um nome
  definido com `eval` pode ser chamado diretamente na linha seguinte.
- **Erros**: os erros de tipo e de sintaxe devolvem `Err` (não causam panic). Os **panics em tempo de execução**
  do código avaliado (divisão por zero etc.) se propagam como fariam a partir de código escrito diretamente. A
  limpeza de qualquer `unwind-protect` no meio é executada
  ([capítulo 8 da Referência de sintaxe](../syntax.md#8-saídas-não-locais-catch--throw--unwind-protect)).
- **Namespace**: quando executado por `typl file.typl` e dentro de um executável AOT, `eval` avalia no namespace
  do módulo do script (as globais do próprio script são visíveis). O REPL avalia no namespace raiz.
- **Compilação**: tanto `read` quanto `eval` podem ser compilados. Como são tratados nos executáveis AOT, e as
  consequências (as formas passadas a eval são interpretadas), está em
  [Referência de sintaxe 10.2](../syntax.md#102-eval-em-executáveis-aot).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (incluindo os de dentro de `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` podem levar docstrings. A posição segue a regra do CL para cada um:

| Forma | Posição da docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | No início do corpo (depois do tipo de retorno e da cláusula `where`). Só quando pelo menos uma forma do corpo a segue; uma string sozinha continua sendo o valor de retorno |
| `defvar` / `defconstant` | **Depois** do valor inicial: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Logo depois** do nome, antes dos campos/variantes |
| `deftype` | **Logo depois** do nome, antes do tipo: `(deftype meters "doc" i32)` |
| `deftrait` | Logo depois da lista de supertraits, antes dos itens. Uma para o trait inteiro. **Os métodos com implementação padrão** podem pôr sua própria docstring logo antes do corpo |

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `documentation` | `(documentation name)` | (forma especial; `name` é um símbolo simples ou `Type::method`)→`Option<string>` | Devolve a docstring de `name` |

Como `quote`/`compile`, `documentation` é uma forma especial (lê `name` como um nome não avaliado). Ao contrário
do `(documentation 'name 'function)` do CL, não recebe argumento de tipo; em vez disso resolve um nome simples na
ordem **variável → função → tipo → trait → macro** (a mesma prioridade de um identificador simples avaliado como
expressão). A forma `Type::method` procura a docstring de um método associado ou estático.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**O valor é decidido na verificação**: se o nome não se resolver para nenhuma definição, é um erro na
verificação (como referir-se a uma variável não definida). Se se resolver mas não houver docstring, o resultado
é `Option::none`.

**Não incluído**:

- `(setf documentation)` (mudar uma docstring em tempo de execução) não existe.
- Nomes livres qualificados por módulo (`mod::name`; `Type::method` é suportado) não são suportados.
- Uma declaração de método em um `deftrait` **sem corpo** não pode ter docstring. Um literal de string final
  seria ele mesmo o corpo (o valor de retorno) de uma implementação padrão, então não há como distinguir os
  dois.

A informação ao passar o mouse do servidor de linguagem (`typl-lsp`) também mostra docstrings.

## 8. Macros

Como definir macros está em [Referência de sintaxe 3.14](../syntax.md#314-defmacro--definição-de-macros).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Um símbolo novo. Seu nome é `" <prefix><n>"`, em que `n` é `*gensym-counter*`. Um espaço inicial não pode ser escrito no código-fonte, então os vínculos gerados nunca colidem com nomes escritos |
| `*gensym-counter*` | Variável | `int` | O número que `gensym` usa em seguida. Como no CL, pode ser lido e definido |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expande uma chamada de macro em um passo. `none` significa "não é uma chamada de macro" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Repete até deixar de ser uma macro |

`macroexpand-1` devolve um `Option`. O CL relata "se expandiu" como segundo valor de retorno, mas não há valores
múltiplos, então `none` faz esse papel. **Uma macro que se expande em uma chamada a si mesma nunca pode ser
confundida com algo que não é macro.** Um passo de expansão é o mesmo que o verificador de tipos usa, então o
que o programa vê e o que o verificador viu nunca divergem.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none aparece como a lista vazia (Option<Sexpr> é transparente)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

O que o CL tem e esta linguagem não: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` sempre
coincidem, então não há distinção a escolher), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (símbolos não internados; os vínculos são procurados pelo nome, então não
haveria nada a ganhar).

## 9. Vínculos de macros locais (`macrolet` / `symbol-macrolet`)

Ambos são formas especiais que vinculam lexicamente **nomes que não são valores**. Nada resta em tempo de
execução: o que é compilado é a forma expandida do corpo.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Um vínculo de `macrolet` esconde uma macro global de mesmo nome **só durante o corpo**. A lista lambda é a
  mesma de `defmacro` (`&optional`/`&rest`/`&key`).
- **Os irmãos de um mesmo `macrolet` não se enxergam a partir dos seus *corpos*** (como no CL; é a diferença em
  relação a `labels`). As expansões são verificadas no lugar de uso, então `earlier` expandir para
  `(later ...)` funciona: ambos são visíveis naquele lugar.
- Um nome de `symbol-macrolet` entra no ambiente como um vínculo comum. Então um `let` interno esconde o mesmo
  nome, e uma variável externa fica escondida: as regras do CL saem como estão.
- **`setf` escreve na expansão.** `(setf head 42)` é `(setf (get v 0) 42)`.
- As expansões são verificadas no **ambiente do lugar de uso** (não no do lugar do vínculo).

## 10. Outros

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic se for falso. Sem mensagem, `assertion failed: <o teste como foi escrito>` (é uma macro, então pode nomear a própria expressão). Os restarts do CL não existem nesta linguagem |
| `warn` | `(warn control args...)` | `(string,...)→()` | Escreve uma linha prefixada com `WARNING: ` em `*error-output*` e **continua**. Uma forma de relatar algo sem devolver um `Result` e sem encerrar o programa |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Substitui globais só durante `body` e as restaura na saída. O CL escreve isso como `let`, mas `let` nesta linguagem sempre vincula lexicamente, daí o nome separado (o mesmo papel da macro homônima do Emacs Lisp). Restaura-as qualquer que seja a forma de sair do corpo: conclusão normal, `throw`, `panic`, `break`/`return`. **Não é um vínculo por tarefa** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Executa `body` com todas as variáveis de controle da impressora em seus valores padrão e `*read-eval*` como `true` ([Impressão](printing.md#6-controlar-quanto-é-impresso)) |
| `exit` | `(exit code)` | `int→!` | Encerra o processo |
| `dump` | `(dump path)` | `string→bool` | Grava o ambiente atual (informação de tipos mais corpos compilados) em um arquivo. `typl --image <path>` reinicia a partir dele. Só do interpretador ([Referência de sintaxe 10.1](../syntax.md#101-dumps)) |

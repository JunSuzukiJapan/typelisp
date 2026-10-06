<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Streams e arquivos

Traits e métodos de stream, tipos de stream concretos, operações com arquivos e nomes de caminho. Os sockets de
rede também são streams, e são tratados em [Rede](network.md).

## 1. A hierarquia de traits

O que o CL expressa com uma hierarquia de classes é expresso aqui com uma **hierarquia de traits**. Tanto a
direção (entrada / saída) quanto o tipo dos elementos são decididos **estaticamente**, então não é preciso
perguntar em tempo de execução "este stream pode ser lido?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; entrada de caracteres
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; saída de caracteres
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; entrada que pode devolver um caractere
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; entrada de bytes
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; saída de bytes
```

Uma função que lê caracteres aceita qualquer tipo de stream, embutido ou definido pelo usuário, se receber
`(where (CharInput S))` ou `:dyn CharInput`.

## 2. Métodos

Todo método de `CharInput` tem uma implementação padrão. Uma implementação só escreve `read-item`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | O próximo elemento. `none` no fim. **O único método que precisa ser implementado** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | O próximo caractere |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Até a próxima quebra de linha (a quebra é consumida e removida). Uma última linha que não termina em quebra de linha também é devolvida |
| `read-all` | `(read-all s)` | `(S)→string` | Tudo o que resta |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Só um caractere que já está à mão. `none` em vez de esperar |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Coloca até `n` caracteres em `v` e devolve quantos foram realmente lidos. Menos de `n` só no fim |

`listen` está em `InputStream` (o pai de `CharInput`):

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Se a próxima leitura pode ser respondida sem esperar. O padrão é `false`, **o lado que nunca é mentira**: `true` seria um palpite, e um palpite errado faria `read-char-no-hang` bloquear. Todos os streams embutidos o sobrescrevem. **Para streams definidos pelo usuário que não o sobrescrevem, `read-char-no-hang` sempre devolve `none`** |

`PeekInput` (que herda de `CharInput`) acrescenta **devolver um caractere**. Só o próprio stream tem onde guardar
o caractere devolvido, então isso não pode ter implementação padrão e é um trait separado.
`file-stream`/`string-input-stream`/`standard-stream` o implementam, e qualquer outro stream o ganha quando
envolvido com `make-peek-stream` (capítulo 4).

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Faz a próxima leitura devolver `c`. **O único método que precisa ser implementado**. Como no CL, só um caractere é garantido |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Olha o próximo caractere sem consumi-lo |

Da mesma forma, para `CharOutput` uma implementação só escreve `write-item`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Escreve um elemento. **O único método que precisa ser implementado** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Escreve um caractere |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Escreve uma string |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Uma string e uma quebra de linha |
| `terpri` | `(terpri s)` | `(S)→()` | Uma quebra de linha (o nome do CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Uma quebra de linha, exceto no início de uma linha |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Se o próximo caractere escrito vai começar uma linha. O padrão é `false` (então `fresh-line` escreve a quebra: na dúvida, escrever é o lado seguro). Todos os streams embutidos o sobrescrevem |
| `finish-output` | `(finish-output s)` | `(S)→()` | Esvazia o buffer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Escreve todos os caracteres de `v` em ordem |

`at-line-start` lembra **só o que foi escrito por esse stream**. `print`/`println`/`(format true ...)` escrevem na
saída padrão sem passar por `*standard-output*`, então se você misturar os dois, `(fresh-line *standard-output*)`
não sabe das quebras de linha que `println` escreveu. Fique com um deles.

`Stream` é comum a todos os streams:

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Se ainda está aberto |
| `close` | `(close s)` | `(S)→()` | Fecha-o. **O GC não fecha streams**, então faça isso explicitamente (ou com `with-open-file`) |

## 3. Tipos de stream concretos

| Tipo | Como criar um | Traits implementados |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` é uma das três constantes `direction-input` / `direction-output` / `direction-append`. `open-file`
devolve `Err(FileError)` se o arquivo não puder ser aberto (um arquivo inexistente é um resultado comum, não um
panic). O nome do arquivo pode ser uma string ou um `pathname` (`Pathish` no capítulo 9).

`(get-output-stream-string s)` devolve o que foi escrito em um `string-output-stream` e o esvazia. Como no CL,
pode ser tirado mesmo depois de `close`.

**A E/S de bytes** usa `ByteInput`/`ByteOutput`. Eles fixam o `Item` de `InputStream`/`OutputStream` em `int`, do
mesmo jeito que `CharInput`/`CharOutput` o fixam em `char`.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | O próximo byte. `none` no fim do arquivo |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Escreve um byte. Erro fora de 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | A versão de caracteres, em bytes |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Igual ao acima |

O CL decide o tipo dos elementos **na chamada**, como em `(open name :element-type '(unsigned-byte 8))`, mas aqui
o tipo dos elementos é **o tipo** do stream, então o que muda é a função que o abre. Ler bytes de um stream de
caracteres é um erro de tipo (`string-input-stream` não implementa `ByteInput`). Ler um byte logo depois de
devolver um caractere com `unread-char` também é um erro.

## 4. Streams compostos

Todos são `defstruct` da biblioteca padrão e podem ser aninhados.

| Nome | Forma | Descrição |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Escreve em todos de um `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Lê de `in` e escreve em `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Lê de `in` e também escreve em `out` os caracteres lidos |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Lê um `Vector<:dyn CharInput>` um depois do outro |
| `make-peek-stream` | `(make-peek-stream in)` | Acrescenta a devolução de um caractere a qualquer `:dyn CharInput`, tornando-o um `PeekInput` (para `read-sexpr`) |

## 5. Macros

| Nome | Forma | Descrição |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Abrir, executar o corpo, fechar. `Result<valor do corpo, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Lê de uma string |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Devolve o que foi escrito |

## 6. Funções genéricas e operações com arquivos

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Transfere tudo |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Todas as linhas restantes |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Lê um `Sexpr` (o `read` do CL). `Ok(eof)` no fim da entrada, `Ok(datum d)` quando lê um, `Err` se não forem dados. **Consome o caractere de espaço em branco** que terminou o dado (como no CL). `ReadOutcome` não é um `Option<Sexpr>` para que ler a lista vazia `()` e o fim da entrada não sejam o mesmo valor |
| `read-sexpr-preserving-whitespace` | Igual ao acima | Igual ao acima | O mesmo, mas deixa o espaço em branco (o `read-preserving-whitespace` do CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Lê até `ch` e forma uma lista. `ch` é consumido. `Err` se a entrada acabar |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Escreve uma linha por vez |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | O conteúdo inteiro |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Todas as linhas |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Grava-o |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Se existe |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Apagar, renomear (os argumentos são `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | O caminho absoluto com os links simbólicos e `.`/`..` resolvidos. `Err` se não existir |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | A hora da última modificação. É **tempo universal**, então `decode-universal-time` ([Tempo](system.md#2-decodificar-e-codificar-datas)) consegue lê-la |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | O nome de login do dono. `Err` se o arquivo não existir, `Ok(none)` se o uid do dono não tiver entrada no banco de senhas: os dois casos que o CL distingue são mantidos separados |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Se é um diretório. **Também `false` se não existir**; use `probe-file` para distinguir os dois casos |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Lista o conteúdo por truename (o caminho absoluto com os links simbólicos resolvidos, como em `truename`). Links simbólicos cujo destino não existe ficam de fora. `.`/`..` ficam de fora. A ordem é a que o SO der |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Cria-o junto com os pais. Dá certo se já existir |

Todo argumento que nomeia um arquivo **pode ser uma string ou um `pathname`**. É o mesmo tratamento dos
designadores de nome de caminho do CL, resolvido pelo trait `Pathish` em vez de um teste de tipo em tempo de
execução (capítulo 9).

O caractere de término de `read-delimited-list` **também termina os tokens**. Ele só tem efeito na
profundidade 0: em `(1 2]` o `]` é lido como parte do próprio texto da lista e relatado como uma lista quebrada.
Não há equivalente ao terceiro argumento `recursive-p` do CL.

## 7. Tornar seu próprio tipo um stream

Escreva um `write-item` e as implementações padrão trazem o resto. Ele também pode entrar em streams compostos.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; todos os métodos restantes são os padrão

(write-line (counter::new 0) "four")   ; write-line, terpri e fresh-line funcionam todos
```

A entrada funciona do mesmo jeito: você só escreve `read-item`. Mesmo um tipo sem devolução própria pode ser
lido com `read` depois de envolvido, como em `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nome | Chamada | Tipo | Descrição |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` lê o caractere `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Devolve o que está registrado |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` lê a sequência de dois caracteres `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Igual ao acima |

`F` é `(fn (string-input-stream char) Option<Sexpr>)`. Como usá-los, quando passam a valer e como diferem do CL
está na [Referência de sintaxe](../syntax.md#11-macros-de-leitura-readtable).

## 9. Nomes de caminho `pathname`

Um nome de arquivo dividido em partes. Contém os componentes de diretório separados por `/`, o nome, o tipo
(extensão) e se começa na raiz.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   dividido no último ponto
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 O trait designador de caminhos `Pathish`

Onde o CL aceita um designador de nome de caminho (uma string ou um nome de caminho), esta linguagem aceita um
`Pathish`. Tanto `string` quanto `pathname` o implementam, e **toda operação com arquivos o recebe de forma
genérica**, então `(open-input "a.txt")` e `(open-input p)` são ambas chamadas comuns (não há teste de tipo em
tempo de execução). O `namestring` de uma string devolve a própria string, então enquanto você passar uma string,
nenhuma análise é feita.

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | A forma de string. Precisa ser implementado |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Converte para `pathname` (a função `pathname` do CL, renomeada porque colidiria com o nome do tipo). Precisa ser implementado |

### 9.2 Funções

| Nome | Forma | Tipo | Descrição |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Divide uma string em partes. Uma `/` final (ou um nome vazio) significa "sem nome", isto é, um diretório |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Constrói um só com os componentes dados (todos `&key`). Um nome ou tipo omitido fica "ausente" e é algo que `merge-pathnames` preenche |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Os componentes de diretório, o mais externo primeiro |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | O nome sem o tipo. `none` para um diretório |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | O que vem depois do último ponto. Um ponto inicial não conta (todo o `.gitignore` é o nome) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Se começa na raiz |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | O diretório pessoal. `none` se não houver `$HOME` (o CL também permite `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | A parte até a última `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Só a parte `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Preenche os componentes que faltam em `p` a partir de `default`. Um `p` relativo vai para baixo do diretório de `default`; um `p` absoluto mantém seu próprio diretório |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | A forma relativa a `default`. Todo o `p` se não estiver sob a base |

Os argumentos de tipo levam todos `(where (Pathish P))`.

## 10. Diferenças em relação ao CL

- **Uma hierarquia de traits, não de classes.** Não existem `input-stream-p` / `output-stream-p`: o tipo
  carrega a direção, então não é uma pergunta a fazer em tempo de execução.
- **`read` tem nomes diferentes para a versão de string e a de stream.** `(read "...")` (corresponde ao primeiro
  valor do `read-from-string` do CL; se você também precisar da posição em que a leitura terminou, use
  `read-from-string`) e `(read-sexpr s)` (o `read` do CL). Uma chamada se resolve para um único tipo de receptor,
  então o mesmo nome não pode ser sobrecarregado.
- **A devolução de caracteres é um trait separado** (`PeekInput`), então os tipos que só precisam de
  `read-char` não são obrigados a implementar `unread-char`.
- **O fechamento é explícito.** O GC não fecha streams (o GC roda em momentos imprevisíveis, então deixar isso
  com ele tornaria imprevisível também o momento do fechamento). Usar `with-open-file` é o jeito seguro.
- **Os nomes de caminho não têm componentes de host, dispositivo nem versão.** Não há nomes de caminho com
  curingas nem nomes de caminho lógicos (`logical-pathname`). O separador é sempre `/`.
- **A função `pathname` é `to-pathname`**, porque tipos, traits e funções compartilham um namespace.
- **Não há correspondência por curingas**, então `directory` é uma função que "lista o conteúdo daquele
  diretório" e nada mais. O `directory` do CL faz correspondência com um padrão de nome de caminho.

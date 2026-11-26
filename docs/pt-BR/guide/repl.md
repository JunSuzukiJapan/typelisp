<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# O REPL

Iniciar `typl` sem argumentos abre o REPL (modo interativo). Este documento explica o que o REPL
oferece além de avaliar expressões: seus comandos, a ajuda durante a digitação, o acesso a
resultados anteriores, como interromper um cálculo e como investigar depois de um erro.

## Iniciar e sair

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Sai-se com `:quit` (abreviado `:q`) ou `Ctrl+D`. O histórico de entrada é guardado em
`~/.typl_history`, de modo que na próxima vez ele pode ser recuperado com `Ctrl+P` / `Ctrl+N` ou
`Ctrl+R` (busca no histórico).

Ao iniciar, antes do primeiro prompt, o seguinte é executado em ordem. Nenhum desses passos mostra
os valores que produz.

1. O conteúdo de `~/.typlrc`, se existir. Com `--no-init` ele não é lido.
2. `--eval FORM` e `--load FILE` da linha de comando, na ordem em que foram escritos. Ambos podem
   ser dados quantas vezes se quiser.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` e `--no-init` servem só para iniciar o REPL. Passá-los ao executar um arquivo é
um erro.

O REPL começa no módulo `user`. Ao passar para outro módulo com `(in-module foo)`, o prompt mostra
esse módulo.

```
typl> (in-module foo)
typl user::foo>
```

## Comandos do REPL

Um `:nome` escrito no início de uma linha é um comando do REPL. `:help` lista os comandos. Um
`:nome` que não está na lista não é um comando; é avaliado como uma palavra-chave comum.

| Comando | Significado |
|---|---|
| `:help` (`:h`, `:?`) | listar os comandos |
| `:type FORM` (`:t`) | mostrar o tipo de FORM sem avaliá-lo |
| `:describe NAME` (`:d`) | mostrar o tipo de definição, seu tipo, campos ou variantes, métodos, os traits que implementa, onde foi definida e sua string de documentação |
| `:apropos TEXT` | procurar em todos os módulos as definições cujo nome contém TEXT (sem diferenciar maiúsculas e minúsculas) |
| `:src NAME` | mostrar o código-fonte de uma definição |
| `:who-calls NAME` | mostrar as funções e métodos que chamam a função ou método NAME |
| `:inspect FORM` (`:i`) | avaliar FORM e abrir seu valor de acordo com o tipo |
| `:backtrace` (`:bt`) | mostrar as chamadas em andamento quando a linha anterior falhou, da mais interna para fora |
| `:locals [N]` | mostrar os argumentos da chamada N de `:backtrace` (a 0 se omitido) |
| `:cd [DIR]` | mudar a pasta atual para DIR (a pasta pessoal se omitido) |
| `:pwd` | mostrar a pasta atual |
| `:quit` (`:exit`, `:q`) | sair do REPL |

NAME pode ser um nome como `inner` ou um caminho como `std::seq::reverse` ou `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Soma 1 a x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Soma 1 a x.
typl> :src inner
(defun inner ((x int) (s string)) int "Soma 1 a x." (+ x 1))
```

`:src` mostra uma definição carregada de um arquivo a partir desse arquivo, e uma definição
digitada no REPL a partir do texto digitado. As definições da biblioteca padrão não têm a
localização no código-fonte registrada, então o código delas não pode ser mostrado.

`:who-calls` procura nos corpos das funções da sessão atual. Para uma função genérica, encontra só
os usos que foram instanciados com tipos concretos.

`:cd` muda a pasta onde `(load ...)` procura, mas `use` continua procurando arquivos na pasta em
que o REPL foi iniciado.

## Abrir um valor (`:inspect`)

`:inspect` abre um valor de acordo com o tipo. Um struct se abre em seus campos, um enum
(inclusive `Option`) nos campos de sua variante, um `Vector` em seus elementos e uma `HashTable`
em suas chaves e valores. Um número entra nessa parte, `u` sobe um nível e `q` termina.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Listas e `Sexpr` não são abertos; são mostrados como estão.

## Ajuda durante a digitação

Ao digitar em um terminal, o seguinte está disponível.

- `Tab` completa nomes (definições e sintaxe como `if` e `defun`), caminhos com `::`, comandos do
  REPL no início da linha e, dentro de uma string, nomes de arquivo.
- Depois do nome de uma função seguido de um espaço, o tipo da função aparece esmaecido no fim da
  linha.
- O parêntese que corresponde ao que está ao lado do cursor é destacado.
- Se Enter for pressionado antes de uma forma terminar, a linha seguinte começa recuada de acordo
  com o número de parênteses abertos.

## Resultados anteriores (`:*`, `:**`, `:***`)

No REPL, `:*` é o último valor mostrado, `:**` o anterior e `:***` o anterior a esse. Cada um tem o
tipo do seu valor.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Uma entrada que não mostrou valor (como uma definição) ou que falhou não faz os resultados
  avançarem.
- Escrever `:**` ou `:***` antes de haver tantos resultados é um erro.
- Um `':*` com quote é uma palavra-chave comum.
- Um `:*` escrito no corpo de uma função continua se referindo ao resultado do momento em que a
  função foi definida. O `*` de Common Lisp lê o valor no momento da chamada, mas o typelisp o fixa
  na definição para que ele tenha um tipo.
- Fora do REPL (ao executar um arquivo, ou em `~/.typlrc`, `--load` e `--eval`), `:*` é uma
  palavra-chave comum.

## Interromper um cálculo (`Ctrl+C`)

Pressionar `Ctrl+C` durante uma avaliação interrompe esse cálculo e volta ao prompt. O que foi
definido antes continua definido. Isso vale tanto para um laço no interpretador quanto para um
laço em código compilado ou uma espera em `sleep` ou `wait`. As limpezas dos `unwind-protect` em
andamento são executadas.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` pressionado durante a digitação só descarta a linha que está sendo digitada.

## Investigar depois de um erro (`:backtrace`, `:locals`)

Depois que uma avaliação para com um erro, `:backtrace` (`:bt`) mostra as chamadas de função que
estavam em andamento naquele momento, da mais interna para fora. `:locals N` mostra os argumentos
da chamada N de acordo com os tipos declarados.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Numa chamada feita a partir da última expressão de uma função (uma chamada de cauda), a função
  que chama não é mostrada. É isso que permite que um laço recursivo de cauda use o espaço de uma
  única chamada, não importa quantas voltas dê.
- Chamadas dentro de funções compiladas e funções anônimas (`lambda`) não são mostradas.
- `:locals` mostra só os argumentos, não as variáveis ligadas com `let`.
- Avaliar a linha seguinte descarta o registro do erro anterior.

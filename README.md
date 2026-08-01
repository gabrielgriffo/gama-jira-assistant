<div align="center">
  <h1>Gama</h1>

  <p>
    Gama é um aplicativo desktop multiplataforma que reúne suas tarefas do Jira e seus registros de ponto do PontoMais para ajudar a acompanhar e lançar suas horas de trabalho.<br>
    Construído com Tauri 2 e Angular 20.<br>
    <sub>Projeto independente, sem qualquer vínculo com a Atlassian ou a PontoMais. Veja o <a href="#aviso-legal">Aviso Legal</a>.</sub>
  </p>
</div>

## Tecnologias

- **Interface:** Angular 20 e TypeScript
- **Aplicativo desktop:** Rust e Tauri 2
- **Integração com as APIs:** [`reqwest`](https://crates.io/crates/reqwest)
- **Banco de dados local:** SQLite via [`sqlx`](https://crates.io/crates/sqlx)
- **Armazenamento de credenciais:** [`keyring`](https://crates.io/crates/keyring), utilizando o armazenamento seguro nativo do sistema operacional, com [`zeroize`](https://crates.io/crates/zeroize) para apagar os segredos da memória após o uso

## Executando localmente

Pré-requisitos:

- [Node.js](https://nodejs.org/) 20 ou superior
- [Rust](https://www.rust-lang.org/tools/install) com a toolchain `stable`
- [Pré-requisitos do Tauri](https://v2.tauri.app/start/prerequisites/) para o sistema operacional utilizado
- No Linux: `gnome-keyring` ou outro provedor compatível com o padrão Secret Service para o armazenamento seguro das credenciais

```bash
# Instalar as dependências
npm install

# Iniciar o ambiente de desenvolvimento (Angular + Tauri)
npm run tauri dev

# Gerar a compilação de produção
npm run tauri build
```

Os arquivos de distribuição são gerados em `src-tauri/target/release/bundle/`.

## Estrutura do Projeto

```
├── src/                              # Aplicação Angular
│   └── app/
│       ├── components/               # Componentes de interface reutilizáveis
│       ├── core/                     # Feedback, modelos compartilhados e ponte com o Tauri
│       ├── features/                 # Páginas da aplicação (tarefas, worklogs, configurações)
│       ├── services/                 # Serviços e regras de negócio
│       ├── shared/                   # Componentes de UI e utilitários de data compartilhados
│       ├── app.component.ts          # Componente raiz
│       ├── app.config.ts             # Configuração da aplicação
│       └── app.routes.ts             # Configuração das rotas
│
└── src-tauri/                        # Aplicação Rust/Tauri
    ├── src/
    │   ├── domain/                   # Entidades, regras de validação e portas
    │   ├── application/              # Casos de uso (Jira, worklogs, sincronização com o PontoMais)
    │   ├── infrastructure/           # Clientes Jira/PontoMais, SQLite, keyring e preferências
    │   ├── presentation/commands/    # Comandos Tauri expostos ao frontend
    │   ├── lib.rs                    # Inicialização e registro dos comandos Tauri
    │   └── main.rs                   # Ponto de entrada
    │
    └── tauri.conf.json               # Configuração e empacotamento
```

## Privacidade e Armazenamento de Dados

O Gama é um aplicativo de execução local. Ele não utiliza servidores próprios, backends intermediários, telemetria ou mecanismos de coleta de dados.

* As requisições são enviadas diretamente da sua máquina para as APIs do Jira Cloud e do PontoMais, utilizando exclusivamente as suas próprias credenciais.
* Tokens e senhas são armazenados no keyring nativo do sistema operacional. Essas informações não são expostas ao webview e não são gravadas em arquivos do aplicativo.
* Tarefas, worklogs e registros de ponto sincronizados ficam em um banco SQLite local e permanecem somente na sua máquina.
* No PontoMais, o aplicativo é somente leitura: ele consulta registros já existentes na sua conta e não registra, altera ou exclui nenhuma marcação de ponto.
* No Jira, o aplicativo lê suas issues, históricos de alteração e worklogs, e cria worklogs apenas quando você os confirma explicitamente.

## Aviso Legal

O Gama é um projeto independente, pessoal e sem fins lucrativos. Ele não possui qualquer afiliação, associação, autorização, endosso ou patrocínio da Atlassian ou da PontoMais, de suas subsidiárias ou empresas afiliadas.

As marcas e os nomes mencionados pertencem aos seus respectivos titulares e são utilizados exclusivamente de forma descritiva, para indicar a compatibilidade do aplicativo.

O acesso às APIs do Jira e do PontoMais é realizado utilizando exclusivamente as credenciais fornecidas pelo próprio usuário e apenas para acessar os dados da sua própria conta.

O uso deste aplicativo pode conflitar com os Termos de Uso da plataforma PontoMais. A responsabilidade pelo uso é inteiramente do usuário, incluindo eventuais medidas que o provedor possa adotar em relação à conta utilizada. Recomenda-se consultar os termos vigentes e, em caso de dúvida, a área de RH da empresa.

O software é fornecido "como está", sem garantias de qualquer natureza. O Gama não substitui o sistema oficial de registro de ponto: os dados e valores exibidos têm caráter exclusivamente informativo, e a fonte oficial dos registros de jornada continua sendo o PontoMais.

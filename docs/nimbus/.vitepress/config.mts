import { defineConfig } from 'vitepress'

// Site de documentação do Nimbus. Build: `npx vitepress build docs/nimbus`.
// Para publicar em https://push-in.github.io/pam/nimbus/, ajuste `base` para '/pam/nimbus/'.
export default defineConfig({
  lang: 'pt-BR',
  title: 'Nimbus',
  description: 'Framework PHP para APIs sobre o runtime PAM',
  base: '/',
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    nav: [
      { text: 'Guia', link: '/primeiros-passos' },
      { text: 'API pública', link: 'https://github.com/push-in/pam/blob/main/packages/nimbus/docs/PUBLIC-API.md' },
      { text: 'Changelog', link: 'https://github.com/push-in/pam/blob/main/packages/nimbus/CHANGELOG.md' },
    ],
    sidebar: [
      {
        text: 'Começo',
        items: [
          { text: 'Índice', link: '/' },
          { text: 'Primeiros passos', link: '/primeiros-passos' },
          { text: 'Instalação e estrutura', link: '/instalacao' },
          { text: 'Configuração tipada', link: '/configuracao' },
          { text: 'Console e comandos', link: '/console' },
        ],
      },
      {
        text: 'HTTP',
        items: [
          { text: 'Rotas e middlewares', link: '/rotas' },
          { text: 'Controllers e container', link: '/controllers' },
          { text: 'Input e validação', link: '/validacao' },
          { text: 'Recursos JSON e paginação', link: '/respostas' },
          { text: 'Erros (problem+json)', link: '/erros' },
          { text: 'OpenAPI e clientes', link: '/openapi' },
          { text: 'Cliente HTTP', link: '/http-client' },
        ],
      },
      {
        text: 'Dados',
        items: [
          { text: 'Banco de dados (Arcano)', link: '/banco' },
          { text: 'Factories e seeders', link: '/factories' },
          { text: 'Cache e tags', link: '/cache' },
          { text: 'Redis, cache e rate limit', link: '/redis' },
          { text: 'Multi-tenant', link: '/tenancy' },
          { text: 'Storage e URLs assinadas', link: '/storage' },
        ],
      },
      {
        text: 'Segurança',
        items: [
          { text: 'Autenticação', link: '/autenticacao' },
          { text: 'Autorização (Gate e policies)', link: '/autorizacao' },
          { text: 'Hash, Crypt e cabeçalhos', link: '/seguranca' },
        ],
      },
      {
        text: 'Trabalho em segundo plano',
        items: [
          { text: 'Filas e agendador', link: '/filas' },
          { text: 'Eventos e broadcasting', link: '/eventos' },
          { text: 'Notificações', link: '/notificacoes' },
          { text: 'E-mail', link: '/email' },
        ],
      },
      {
        text: 'Web',
        items: [
          { text: 'Sessão, cookies e CSRF', link: '/sessao' },
          { text: 'Views .nim.php', link: '/views' },
          { text: 'Tradução', link: '/traducao' },
          { text: 'Painel administrativo', link: '/admin' },
        ],
      },
      {
        text: 'Operação',
        items: [
          { text: 'Testes', link: '/testes' },
          { text: 'Logs e observabilidade', link: '/observabilidade' },
          { text: 'Resiliência e transações', link: '/resiliencia' },
          { text: 'Deploy', link: '/deploy' },
          { text: 'Desempenho', link: '/desempenho' },
          { text: 'Migração do Laravel', link: '/migracao-laravel' },
          { text: 'Regras Rector', link: 'https://github.com/push-in/pam/tree/main/packages/nimbus-rector' },
        ],
      },
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/push-in/pam' }],
    editLink: {
      pattern: 'https://github.com/push-in/pam/edit/main/docs/nimbus/:path',
      text: 'Editar esta página',
    },
    outline: { label: 'Nesta página', level: [2, 3] },
    docFooter: { prev: 'Anterior', next: 'Próxima' },
  },
})

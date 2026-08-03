FROM node@sha256:f70403e87646dc51b45295f4b8b70cdad0b63d2297c4c9899119b03f7af7a6b3

WORKDIR /benchmark
COPY benchmarks/laravel/node-server.mjs ./server.mjs

USER node
EXPOSE 8080
CMD ["node", "server.mjs"]

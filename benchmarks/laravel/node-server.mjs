import cluster from 'node:cluster';
import { createServer } from 'node:http';

const workers = Number.parseInt(process.env.BENCH_WORKERS ?? '4', 10);
if (!Number.isSafeInteger(workers) || workers < 1 || workers > 64) {
  throw new RangeError('BENCH_WORKERS must be an integer between 1 and 64');
}

if (cluster.isPrimary) {
  for (let index = 0; index < workers; index += 1) {
    cluster.fork();
  }
  cluster.on('exit', (_worker, code, signal) => {
    throw new Error(`Node benchmark worker exited: code=${code} signal=${signal}`);
  });
} else {
  const body = '{"message":"pong"}';
  createServer((request, response) => {
    if (request.method !== 'GET' || request.url !== '/api/ping') {
      response.writeHead(404, { 'content-type': 'application/json' });
      response.end('{"message":"not found"}');
      return;
    }
    response.writeHead(200, {
      'content-length': Buffer.byteLength(body),
      'content-type': 'application/json',
    });
    response.end(body);
  }).listen(8080, '0.0.0.0');
}

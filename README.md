# Demo instructions

1. Start Backend: cargo run in backend directory
2. Start some containers
    * docker run -d --name demo-web nginx:alpine
    * docker run -d --name demo-cache redis:alpine 
    * docker run -d --name demo-db postgres:alpine
3. Start Frontend: npm start in docker-web-ui
4. Visit: http://localhost:3000 to see live container topology!
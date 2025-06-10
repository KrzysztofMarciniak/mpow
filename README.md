# Minimal Proof of Work
🚀 Minimal Proof of Work ⛏️ 🦀 Written in Rust 

![solving](screenshot.png)

### How it Works:
```mermaid
sequenceDiagram
    participant User
    participant Nginx
    participant Server

    User->>Nginx: GET /
    Nginx->>Server: GET /validate (auth_request)
    alt Valid JWT Cookie
        Server-->>Nginx: 200 OK
        Nginx-->>User: Serve protected content
    else No valid JWT
        Server-->>Nginx: 401 Unauthorized
        Nginx->>Server: GET /get_challenge
        Server-->>Nginx: HTML PoW Challenge
        Nginx-->>User: Challenge page
    end
    
    User->>Nginx: POST /post_nonce (nonce submission)
    Nginx->>Server: POST /post_nonce
    alt Nonce valid
        Server-->>Nginx: "PoW verified" + Set-Cookie
        Nginx-->>User: Success + JWT Cookie
        User->>Nginx: GET /
        Nginx-->>User: Protected content (authenticated)
    else Nonce invalid
        Server-->>Nginx: "Invalid nonce" 
        Nginx-->>User: Error response
    end
```

### In details:
```mermaid
graph TD
    A[User visits /] --> B[Nginx receives request]
    B --> C[Nginx makes auth_request to /validate]
    
    C --> D{Valid JWT cookie?}
    D -->|Yes| E[Nginx serves protected content]
    D -->|No| F[Nginx redirects to /get_challenge]
    
    F --> G[Server generates new token and challenge]
    G --> H[Server stores challenge in memory]
    H --> I[Server responds with HTML + challenge]
    I --> J[Nginx serves challenge page to user]

    J --> K[User solves challenge with nonce]
    K --> L[User submits nonce via POST /post_nonce]
    L --> M[Nginx proxies to server]

    M --> N{Is token in challenge map?}
    N -->|No| O[Respond: 403 Forbidden - No active challenge]

    N -->|Yes| P{Is challenge expired?}
    P -->|Yes| Q[Delete challenge] 
    Q --> O

    P -->|No| R{Too many attempts?}
    R -->|Yes| S[Respond: 429 Too Many Requests]

    R -->|No| T[Server hashes challenge + nonce]
    T --> U{Hash starts with required zeros?}

    U -->|No| V[Increment attempts]
    V --> W[Respond: 403 Forbidden - Invalid nonce]

    U -->|Yes| X[Generate JWT token]
    X --> Y[Delete challenge from memory]
    Y --> Z[Set secure cookie with JWT]
    Z --> AA[Respond: 200 OK - Access granted]
    AA --> BB[Nginx passes cookie to user]
    BB --> CC[User can now access protected content]
```

## 🐳 Docker Deployment

### Architecture
The application uses a multi-container setup with Docker Compose:

- **mpow-auth**: Rust application container (authentication service)
- **nginx**: Reverse proxy with auth_request module for protection
- **mpow-network**: Bridge network for container communication

### Quick Start

```bash
docker-compose up -d
```

```bash
curl http://localhost
```

### Container Details

#### 1. Rust Application Container (`mpow-auth`)
- **Base Image**: `rust` (multi-stage build)
- **Build Stage**: Compiles the Rust application
- **Runtime Stage**: Runs the compiled binary
- **Port**: 3000 (internal)
- **Environment**: `RUST_LOG=info`

#### 2. Nginx Reverse Proxy (`mpow-nginx`)
- **Base Image**: `nginx:alpine`
- **Ports**: 80 (HTTP), 443 (HTTPS)
- **Features**:
  - `auth_request` module for authentication
  - Reverse proxy to Rust backend
  - Static file serving for protected content

### File Structure
```
.
├── Dockerfile                 # Multi-stage Rust build
├── docker-compose.yml         # Container orchestration
├── nginx/
│   ├── nginx.conf            # Nginx configuration
│   ├── html/
│   │   └── private/
│   │       └── index.html    # Protected content
└── src/                      # Rust source code
```

### Nginx Configuration Highlights

#### Authentication Flow
```nginx
location / {
    auth_request /validate;           # Check authentication
    error_page 401 = /get_challenge;  # Redirect if unauthorized
    try_files /private/index.html =404;
}
```

#### Internal Validation
```nginx
location = /validate {
    internal;                         # Internal use only
    proxy_pass http://mpow-auth/validate;
    proxy_pass_request_body off;
    proxy_set_header Cookie $http_cookie;
}
```

#### Challenge & Nonce Handling
```nginx
location = /get_challenge {
    proxy_pass http://mpow-auth/get_challenge;
    proxy_pass_header Set-Cookie;     # Pass JWT cookies
}

location = /post_nonce {
    proxy_pass http://mpow-auth/post_nonce;
    proxy_pass_header Set-Cookie;     # Pass JWT cookies
}
```

### Ports

| Service | Internal Port | External Port | Description |
|---------|---------------|---------------|-------------|
| mpow-auth | 3000 | - | Rust authentication service |
| nginx | 80 | 80 | HTTP web server |

### Development

#### Build and run locally:
```bash
docker-compose up --build
```

#### View logs:
```bash
docker-compose logs -f mpow-auth
docker-compose logs -f nginx
```

#### Stop services:
```bash
docker-compose down
```

### API Endpoints:
- `GET /` - Protected content (requires authentication)
- `GET /get_challenge` - Returns HTML page with PoW challenge
- `POST /post_nonce` - Submit nonce solution for verification  
- `GET /validate` - Internal endpoint for nginx auth_request

### Security Features:
- JWT tokens with expiration
- Challenge expiration (prevents replay attacks)
- Rate limiting (max attempts per challenge)
- Secure HTTP-only cookies
- CSRF protection via SameSite cookies
- Nginx reverse proxy protection
- Internal-only validation endpoints

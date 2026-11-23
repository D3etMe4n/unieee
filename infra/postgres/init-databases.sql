-- Initialize databases and extensions for unieee
CREATE EXTENSION IF NOT EXISTS pgcrypto;

SELECT 'CREATE DATABASE keycloak'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'keycloak')\gexec

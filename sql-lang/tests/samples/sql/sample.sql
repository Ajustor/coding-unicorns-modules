-- Schema
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT now()
);

/* Query */
SELECT u.id, count(o.id) AS orders
FROM users u
LEFT JOIN orders o ON o.user_id = u.id
WHERE u.email LIKE '%@example.org' AND o.total > 10.5
GROUP BY u.id
HAVING count(o.id) >= 2
ORDER BY orders DESC
LIMIT 10;

INSERT INTO users (email) VALUES ('a@example.org') RETURNING id;
UPDATE users SET email = NULL WHERE id = 3;

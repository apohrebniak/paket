# Paket

Like the [Pocket](https://en.wikipedia.org/wiki/Pocket_(service)) used to be, but the article will die.🪦

### Save an Article

```http
PUT /save
Content-Type: application/x-www-form-urlencoded

url=https://example.com/article
```

### Delete an Article

```http
POST /delete
Content-Type: application/x-www-form-urlencoded

guid=<guid>
```

### Get HTML feed

```http
GET /feed.html
```

### Get RSS Feed

```http
GET /feed.xml
```

```
Usage: paket [-n <name>] [-d <desc>] -l <link> [--db <db>] [-p <port>] [--ttl <ttl>]

Paket: read before it goes away

Options:
  -n, --name        feed name
  -d, --desc        feed description
  -l, --link        feed HTTP url
  --db              database file
  -p, --port        server port
  --ttl             time to live in days
  -h, --help        display usage information
```


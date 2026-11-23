-- Spring Modulith 2.1.x Event Publication Registry (Transactional Outbox)
CREATE TABLE IF NOT EXISTS event_publication (
    id                      UUID NOT NULL,
    completion_date         TIMESTAMP(9) WITH TIME ZONE,
    event_type              VARCHAR(512) NOT NULL,
    listener_id             VARCHAR(512) NOT NULL,
    publication_date        TIMESTAMP(9) WITH TIME ZONE NOT NULL,
    serialized_event        TEXT NOT NULL,
    status                  VARCHAR(20),
    completion_attempts     INT,
    last_resubmission_date  TIMESTAMP(9) WITH TIME ZONE,
    PRIMARY KEY (id)
);

-- Fast lookup for incomplete events needing retry
CREATE INDEX IF NOT EXISTS idx_event_pub_incomplete
    ON event_publication (publication_date)
    WHERE completion_date IS NULL;

-- Fast cleanup of completed events to keep table compact in PostgreSQL shared buffers
CREATE INDEX IF NOT EXISTS idx_event_pub_completed
    ON event_publication (completion_date)
    WHERE completion_date IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_event_pub_listener_type
    ON event_publication (listener_id, event_type);

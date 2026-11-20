#pragma once

#include <QHash>
#include <QObject>

class KIdleTime final : public QObject
{
    Q_OBJECT

public:
    static KIdleTime *instance()
    {
        static KIdleTime idle;
        return &idle;
    }

    QHash<int, int> idleTimeouts() const
    {
        return timeouts_;
    }

    int addIdleTimeout(int milliseconds)
    {
        const int identifier = nextIdentifier_++;
        timeouts_.insert(identifier, milliseconds);
        return identifier;
    }

    void removeIdleTimeout(int identifier)
    {
        timeouts_.remove(identifier);
    }

Q_SIGNALS:
    void timeoutReached(int identifier, int milliseconds);

private:
    QHash<int, int> timeouts_;
    int nextIdentifier_ = 0;
};

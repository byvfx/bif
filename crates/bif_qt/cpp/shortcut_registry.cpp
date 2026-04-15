#include "shortcut_registry.h"

#include <QMutex>
#include <QMutexLocker>
#include <QSettings>

namespace bif_qt::shortcuts {

namespace {

QMutex& registry_mutex() {
    static QMutex m;
    return m;
}

QHash<QString, QKeySequence>& registry() {
    static QHash<QString, QKeySequence> r;
    return r;
}

QString settings_key(const char* action_id) {
    return QStringLiteral("shortcuts/%1").arg(QLatin1String(action_id));
}

}  // namespace

QKeySequence lookup(const char* action_id, const QKeySequence& default_sequence) {
    {
        QMutexLocker lock(&registry_mutex());
        registry().insert(QLatin1String(action_id), default_sequence);
    }

    QSettings settings;
    const auto override_str = settings.value(settings_key(action_id)).toString();
    if (!override_str.isEmpty()) {
        const QKeySequence parsed(override_str);
        if (!parsed.isEmpty()) {
            return parsed;
        }
    }
    return default_sequence;
}

void set_override(const char* action_id, const QKeySequence& sequence) {
    QSettings settings;
    if (sequence.isEmpty()) {
        settings.remove(settings_key(action_id));
    } else {
        settings.setValue(settings_key(action_id), sequence.toString());
    }
}

void clear_override(const char* action_id) {
    set_override(action_id, QKeySequence());
}

QHash<QString, QKeySequence> registered_defaults() {
    QMutexLocker lock(&registry_mutex());
    return registry();
}

}  // namespace bif_qt::shortcuts

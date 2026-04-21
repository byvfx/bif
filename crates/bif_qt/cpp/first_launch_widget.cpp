#include "first_launch_widget.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QSettings>
#include <QSizePolicy>
#include <QSpacerItem>
#include <QVBoxLayout>

namespace {

// Build a big card-style button. Returns a QPushButton with its
// preferred size + visual styling for the cards. Phase G refines
// the card design.
QPushButton* make_card_button(const QString& title, const QString& subtitle, QWidget* parent) {
    auto* btn = new QPushButton(parent);
    btn->setMinimumSize(280, 160);
    btn->setMaximumSize(360, 200);
    btn->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    btn->setText(QStringLiteral("%1\n\n%2").arg(title).arg(subtitle));
    btn->setStyleSheet(QStringLiteral(
        "QPushButton {"
        "  background-color: rgba(42, 47, 54, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  border-radius: 8px;"
        "  color: rgba(220, 222, 226, 255);"
        "  font-size: 14px;"
        "  padding: 18px;"
        "  text-align: center;"
        "}"
        "QPushButton:hover {"
        "  background-color: rgba(51, 56, 64, 255);"
        "  border-color: rgba(74, 144, 217, 200);"
        "}"
        "QPushButton:pressed {"
        "  background-color: rgba(50, 95, 145, 255);"
        "}"));
    return btn;
}

}  // namespace

FirstLaunchWidget::FirstLaunchWidget(QWidget* parent)
    : QWidget(parent), m_recent_list(nullptr) {
    setObjectName(QStringLiteral("first_launch_widget"));
    setStyleSheet(QStringLiteral("background-color: rgba(26, 29, 33, 255);"));

    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(48, 64, 48, 64);
    outer->setSpacing(24);

    // Title
    auto* title = new QLabel(QStringLiteral("Welcome to BIF"), this);
    title->setAlignment(Qt::AlignCenter);
    title->setStyleSheet(QStringLiteral(
        "color: rgba(220, 222, 226, 255);"
        "font-size: 28px;"
        "font-weight: 300;"));
    outer->addWidget(title);

    auto* tagline = new QLabel(
        QStringLiteral("USD Orchestration for VFX — layer-aware editing + procedural assembly"),
        this);
    tagline->setAlignment(Qt::AlignCenter);
    tagline->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255);"
        "font-size: 13px;"));
    outer->addWidget(tagline);

    outer->addItem(new QSpacerItem(0, 16, QSizePolicy::Minimum, QSizePolicy::Fixed));

    // Two cards side-by-side
    auto* cards_row = new QHBoxLayout();
    cards_row->setSpacing(20);
    cards_row->addStretch();

    auto* new_btn = make_card_button(
        QStringLiteral("📄  New Stage"),
        QStringLiteral("Start with an empty USD stage."),
        this);
    QObject::connect(new_btn, &QPushButton::clicked,
        this, &FirstLaunchWidget::newStageClicked);
    cards_row->addWidget(new_btn);

    auto* open_btn = make_card_button(
        QStringLiteral("📂  Open Stage..."),
        QStringLiteral("Open an existing .usd / .usda / .usdc / .usdz."),
        this);
    QObject::connect(open_btn, &QPushButton::clicked,
        this, &FirstLaunchWidget::openStageClicked);
    cards_row->addWidget(open_btn);

    cards_row->addStretch();
    outer->addLayout(cards_row);

    outer->addItem(new QSpacerItem(0, 32, QSizePolicy::Minimum, QSizePolicy::Fixed));

    // Recent Stages
    auto* recent_label = new QLabel(QStringLiteral("Recent Stages"), this);
    recent_label->setAlignment(Qt::AlignCenter);
    recent_label->setStyleSheet(QStringLiteral(
        "color: rgba(140, 145, 155, 255);"
        "font-size: 11px;"
        "letter-spacing: 1px;"
        "text-transform: uppercase;"));
    outer->addWidget(recent_label);

    m_recent_list = new QListWidget(this);
    m_recent_list->setMaximumWidth(520);
    m_recent_list->setMaximumHeight(180);
    m_recent_list->setStyleSheet(QStringLiteral(
        "QListWidget {"
        "  background-color: rgba(34, 38, 44, 255);"
        "  border: 1px solid rgba(60, 65, 75, 255);"
        "  border-radius: 6px;"
        "  color: rgba(220, 222, 226, 255);"
        "  padding: 4px;"
        "}"
        "QListWidget::item { padding: 6px 8px; }"
        "QListWidget::item:hover { background-color: rgba(51, 56, 64, 255); }"
        "QListWidget::item:selected { background-color: rgba(74, 144, 217, 100); }"));
    auto* list_row = new QHBoxLayout();
    list_row->addStretch();
    list_row->addWidget(m_recent_list);
    list_row->addStretch();
    outer->addLayout(list_row);

    QObject::connect(m_recent_list, &QListWidget::itemActivated,
        this, [this](QListWidgetItem* item) {
            if (item) emit recentStageActivated(item->text());
        });

    outer->addStretch();

    refresh_recents();
}

FirstLaunchWidget::~FirstLaunchWidget() = default;

void FirstLaunchWidget::refresh_recents() {
    m_recent_list->clear();
    QSettings settings;
    const auto recents = settings.value(QStringLiteral("recent_stages"))
                              .toStringList();
    if (recents.isEmpty()) {
        auto* item = new QListWidgetItem(
            QStringLiteral("(no recent stages — open one above)"),
            m_recent_list);
        item->setFlags(Qt::NoItemFlags);
    } else {
        for (const auto& path : recents) {
            new QListWidgetItem(path, m_recent_list);
        }
    }
}

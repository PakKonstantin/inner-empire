import SwiftUI

/// Readable names for the shared scale.
///
/// Hand-written rather than generated, and deliberately so: the CSS numbers
/// its spacing steps, which is right for a stylesheet, and `space3` says
/// nothing at a call site. The mapping is here in one place where it can be
/// reviewed, rather than invented in the generator where it would look like it
/// came from the design system.
///
/// These are aliases, not new values. Changing a number means changing
/// `--space-3` in `src/styles/theme.css`, and both platforms follow.
extension DesignTokens {
    /// 4pt — between an icon and its label.
    static var spacingXs: CGFloat { space1 }
    /// 8pt — between controls in a row.
    static var spacingSm: CGFloat { space2 }
    /// 12pt — between rows.
    static var spacingMd: CGFloat { space3 }
    /// 16pt — a screen's side gutter.
    static var spacingLg: CGFloat { space4 }
    /// 24pt — between sections.
    static var spacingXl: CGFloat { space5 }

    static var cornerRadiusSmall: CGFloat { radiusS }
    static var cornerRadiusMedium: CGFloat { radiusM }
    static var cornerRadiusLarge: CGFloat { radiusL }
}

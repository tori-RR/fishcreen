using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal sealed class ToggleSwitch : CheckBox
    {
        private bool darkMode;

        internal bool DarkMode
        {
            get { return darkMode; }
            set
            {
                darkMode = value;
                Invalidate();
            }
        }

        internal ToggleSwitch()
        {
            AutoSize = false;
            Cursor = Cursors.Hand;
            Size = new Size(46, 24);
            SetStyle(
                ControlStyles.AllPaintingInWmPaint |
                ControlStyles.OptimizedDoubleBuffer |
                ControlStyles.ResizeRedraw |
                ControlStyles.UserPaint,
                true);
        }

        protected override void OnCheckedChanged(System.EventArgs eventArgs)
        {
            base.OnCheckedChanged(eventArgs);
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs eventArgs)
        {
            eventArgs.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            eventArgs.Graphics.Clear(Parent == null ? SystemColors.Control : Parent.BackColor);

            Rectangle trackBounds = new Rectangle(1, 2, Width - 2, Height - 4);
            using (GraphicsPath track = CreateRoundedRectangle(trackBounds, trackBounds.Height / 2))
            using (SolidBrush trackBrush = new SolidBrush(
                Checked
                    ? Color.FromArgb(35, 122, 230)
                    : (darkMode
                        ? Color.FromArgb(58, 58, 58)
                        : Color.FromArgb(166, 171, 178))))
            {
                eventArgs.Graphics.FillPath(trackBrush, track);
            }

            int thumbSize = Height - 8;
            int thumbX = Checked ? Width - thumbSize - 4 : 4;
            Rectangle thumbBounds = new Rectangle(thumbX, 4, thumbSize, thumbSize);

            using (SolidBrush thumbBrush = new SolidBrush(
                darkMode ? Color.FromArgb(247, 247, 247) : Color.White))
            {
                eventArgs.Graphics.FillEllipse(thumbBrush, thumbBounds);
            }

            if (Focused)
            {
                ControlPaint.DrawFocusRectangle(eventArgs.Graphics, ClientRectangle);
            }
        }

        private static GraphicsPath CreateRoundedRectangle(Rectangle bounds, int radius)
        {
            int diameter = radius * 2;
            GraphicsPath path = new GraphicsPath();
            path.AddArc(bounds.Left, bounds.Top, diameter, diameter, 180, 90);
            path.AddArc(bounds.Right - diameter, bounds.Top, diameter, diameter, 270, 90);
            path.AddArc(bounds.Right - diameter, bounds.Bottom - diameter, diameter, diameter, 0, 90);
            path.AddArc(bounds.Left, bounds.Bottom - diameter, diameter, diameter, 90, 90);
            path.CloseFigure();
            return path;
        }
    }
}

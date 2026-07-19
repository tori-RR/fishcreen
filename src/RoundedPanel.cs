using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal sealed class RoundedPanel : Panel
    {
        private bool darkMode;
        private int cornerRadius = 18;

        internal bool DarkMode
        {
            get { return darkMode; }
            set
            {
                if (darkMode == value)
                {
                    return;
                }

                darkMode = value;
                BackColor = SurfaceColor;
                Invalidate();
            }
        }

        internal Color SurfaceColor
        {
            get
            {
                return darkMode
                    ? Color.FromArgb(27, 27, 27)
                    : Color.White;
            }
        }

        internal int CornerRadius
        {
            get { return cornerRadius; }
            set
            {
                cornerRadius = value < 4 ? 4 : value;
                UpdateRegion();
                Invalidate();
            }
        }

        internal RoundedPanel()
        {
            BackColor = SurfaceColor;
            SetStyle(
                ControlStyles.AllPaintingInWmPaint |
                ControlStyles.OptimizedDoubleBuffer |
                ControlStyles.ResizeRedraw |
                ControlStyles.UserPaint,
                true);
        }

        protected override void OnResize(System.EventArgs eventArgs)
        {
            base.OnResize(eventArgs);
            UpdateRegion();
        }

        protected override void OnPaintBackground(PaintEventArgs eventArgs)
        {
            eventArgs.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            eventArgs.Graphics.Clear(Parent == null ? SurfaceColor : Parent.BackColor);

            Rectangle bounds = new Rectangle(0, 0, Width - 1, Height - 1);
            using (GraphicsPath path = CreateRoundedRectangle(bounds, cornerRadius))
            using (SolidBrush brush = new SolidBrush(SurfaceColor))
            using (Pen borderPen = new Pen(
                darkMode
                    ? Color.FromArgb(34, 34, 34)
                    : Color.FromArgb(231, 233, 237)))
            {
                eventArgs.Graphics.FillPath(brush, path);
                eventArgs.Graphics.DrawPath(borderPen, path);
            }
        }

        private void UpdateRegion()
        {
            if (Width <= 0 || Height <= 0)
            {
                return;
            }

            using (GraphicsPath path = CreateRoundedRectangle(
                new Rectangle(0, 0, Width, Height),
                cornerRadius))
            {
                Region oldRegion = Region;
                Region = new Region(path);
                if (oldRegion != null)
                {
                    oldRegion.Dispose();
                }
            }
        }

        private static GraphicsPath CreateRoundedRectangle(Rectangle bounds, int radius)
        {
            int diameter = radius * 2;
            diameter = System.Math.Min(
                diameter,
                System.Math.Min(bounds.Width, bounds.Height));

            GraphicsPath path = new GraphicsPath();
            path.AddArc(bounds.Left, bounds.Top, diameter, diameter, 180, 90);
            path.AddArc(bounds.Right - diameter, bounds.Top, diameter, diameter, 270, 90);
            path.AddArc(
                bounds.Right - diameter,
                bounds.Bottom - diameter,
                diameter,
                diameter,
                0,
                90);
            path.AddArc(bounds.Left, bounds.Bottom - diameter, diameter, diameter, 90, 90);
            path.CloseFigure();
            return path;
        }
    }
}

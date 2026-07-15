interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  interactive?: boolean;
}

export function Card({
  interactive = false,
  className = "",
  children,
  ...props
}: CardProps) {
  return (
    <div
      className={`content-panel rounded-[1.4rem] p-5 transition-all duration-200 ${
        interactive ? "cursor-pointer hover:-translate-y-0.5 hover:border-border-default hover:bg-surface-raised hover:shadow-md" : ""
      } ${className}`}
      {...props}
    >
      {children}
    </div>
  );
}

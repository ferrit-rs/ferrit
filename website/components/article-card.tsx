import Image from "next/image";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export function ArticleCard({
  url,
  title,
  description,
  date,
  tags,
  image,
}: {
  url: string;
  title: string;
  description?: string;
  date?: string;
  tags?: string[];
  image: string;
}) {
  return (
    <Card className="h-full transition-colors hover:bg-accent/50">
      <Link href={url}>
        <Image
          src={image}
          alt={title}
          width={1200}
          height={630}
          className="aspect-[1200/630] w-full object-cover"
        />
        <CardHeader>
          {date && (
            <p className="text-xs text-muted-foreground">
              {new Date(date).toLocaleDateString("en-US", {
                year: "numeric",
                month: "long",
                day: "numeric",
              })}
            </p>
          )}
          <CardTitle className="text-lg">{title}</CardTitle>
        </CardHeader>
        {description && (
          <CardContent>
            <p className="text-sm text-muted-foreground">{description}</p>
          </CardContent>
        )}
      </Link>
      {tags && tags.length > 0 && (
        <CardContent className="flex flex-wrap gap-1.5 pt-0">
          {tags.map((tag) => (
            <Badge key={tag} variant="secondary" render={<Link href={`/tags/${tag}`} />}>
              {tag}
            </Badge>
          ))}
        </CardContent>
      )}
    </Card>
  );
}

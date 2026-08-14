export function discordThreadUrl(guildId, threadId) {
  return guildId ? `https://discord.com/channels/${guildId}/${threadId}` : `https://discord.com/channels/@me/${threadId}`;
}

function exportMessage(message) {
  const reactionDataAvailable = Array.isArray(message.reactions);
  const reactions = reactionDataAvailable ? message.reactions.map((reaction) => ({
    emoji: reaction.emoji ? {
      id: reaction.emoji.id ?? undefined,
      name: reaction.emoji.name,
      animated: reaction.emoji.animated ?? undefined,
    } : undefined,
    count: reaction.count ?? 0,
    countDetails: reaction.count_details ? {
      burst: reaction.count_details.burst ?? 0,
      normal: reaction.count_details.normal ?? 0,
    } : undefined,
    reactedByAuthorizedAccount: reaction.me ?? false,
  })) : [];
  return {
    id: message.id,
    url: message.guild_id && message.channel_id && message.id
      ? `https://discord.com/channels/${message.guild_id}/${message.channel_id}/${message.id}`
      : undefined,
    createdAt: message.timestamp,
    updatedAt: message.edited_timestamp ?? undefined,
    content: message.content ?? "",
    author: message.author ? {
      id: message.author.id,
      username: message.author.username,
      globalName: message.author.global_name ?? undefined,
    } : undefined,
    reactionDataAvailable,
    reactions,
    reactionCount: reactionDataAvailable
      ? reactions.reduce((total, reaction) => total + reaction.count, 0)
      : null,
    attachments: (message.attachments ?? []).map((attachment) => ({
      id: attachment.id,
      filename: attachment.filename,
      url: attachment.url,
      contentType: attachment.content_type ?? undefined,
      size: attachment.size,
      description: attachment.description ?? undefined,
    })),
  };
}

export function exportThread(channel) {
  const messages = (channel.messages ?? []).map(exportMessage);
  const reactionDataAvailable = messages.every((message) => message.reactionDataAvailable);
  return {
    id: channel.id,
    guildId: channel.guild_id,
    parentId: channel.parent_id ?? undefined,
    name: channel.name,
    topic: channel.topic ?? undefined,
    type: channel.type,
    url: discordThreadUrl(channel.guild_id, channel.id),
    reactionDataAvailable,
    starterReactionCount: messages[0]?.reactionCount ?? null,
    reactionCount: reactionDataAvailable
      ? messages.reduce((total, message) => total + message.reactionCount, 0)
      : null,
    messages,
  };
}

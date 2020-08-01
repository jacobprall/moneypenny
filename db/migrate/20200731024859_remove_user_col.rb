class RemoveUserCol < ActiveRecord::Migration[5.2]
  def change
    remove_column :users, :avatar_content_type, :gender
  end
end

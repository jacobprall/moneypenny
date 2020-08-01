class RemoveFromUser < ActiveRecord::Migration[5.2]
  def change
    remove_column :users, :provider, :uuid
  end
end

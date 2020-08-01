class RemoveUserCols < ActiveRecord::Migration[5.2]
  def change
    remove_column :users, :uid
    remove_column :users, :gender
  end
end
